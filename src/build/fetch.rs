use crate::source::{SourceFile, SourceLocation};
use crate::util::{asyncify, tempfile_async, PB_STYLE_BYTES};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use futures::stream::FuturesUnordered;
use futures::{TryFutureExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{Client, Url};
use std::fs::{create_dir_all, remove_file, File, Permissions};
use std::io::{self, Read, Seek};
use std::os::unix::prelude::PermissionsExt;
use std::path::{Component, Path};
use std::str::from_utf8;
use tokio::fs::{copy, metadata, File as AsyncFile};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::runtime::Builder as RtBuilder;
use xz2::read::XzDecoder;
use zip::ZipArchive;
use zstd::stream::read::Decoder as ZstDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
  Tar,
  TarGz,
  TarXz,
  TarBz2,
  TarZst,
  Zip,
  Deb,
  // reserved for future use
  #[allow(unused)]
  Ar,
}

impl ArchiveKind {
  fn from_file_name(name: &str) -> Option<(Self, &str)> {
    let mut segments = name.rsplit('.').peekable();
    let (kind, ext_len) = match (segments.next()?, segments.peek()) {
      ("tar", _) => (Self::Tar, 4),
      ("tgz", _) => (Self::TarGz, 4),
      ("txz", _) => (Self::TarXz, 4),
      ("tbz2", _) => (Self::TarBz2, 5),
      ("tzst", _) => (Self::TarZst, 5),
      ("zip", _) => (Self::Zip, 4),
      ("deb", _) => (Self::Deb, 4),
      ("gz", Some(&"tar")) => (Self::TarGz, 7),
      ("xz", Some(&"tar")) => (Self::TarXz, 7),
      ("bz2", Some(&"tar")) => (Self::TarBz2, 8),
      ("zst", Some(&"tar")) => (Self::TarZst, 8),
      _ => return None,
    };
    Some((kind, &name[..name.len() - ext_len]))
  }
}

struct FlowMeter<R: Read> {
  inner: R,
  pb: ProgressBar,
}

impl<R: Read> FlowMeter<R> {
  fn new(inner: R, pb: ProgressBar) -> Self {
    Self { inner, pb }
  }
}

impl<R: Read> Read for FlowMeter<R> {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let result = self.inner.read(buf)?;
    self.pb.inc(result as _);
    Ok(result)
  }
}

impl<R: Read + Seek> Seek for FlowMeter<R> {
  fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
    self.inner.seek(pos)
  }
}

// Taken from ZipArchive::enclosed_name
fn is_safe_name(name: &str) -> bool {
  if name.contains('\0') {
    return false;
  }
  let path = Path::new(name);
  let mut depth = 0usize;
  for component in path.components() {
    match component {
      Component::Prefix(_) | Component::RootDir => return false,
      Component::ParentDir => {
        if depth == 0 {
          return false;
        }
        depth -= 1;
      }
      Component::Normal(_) => depth += 1,
      Component::CurDir => {}
    }
  }
  true
}

fn extract_ar(src: impl Read + Seek, dst: &Path) -> io::Result<()> {
  let mut ar = ar::Archive::new(src);
  while let Some(mut entry) = ar.next_entry().transpose()? {
    let name = from_utf8(entry.header().identifier())
      .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    if !is_safe_name(name) {
      break;
    }
    let path = dst.join(name);
    let parent = path.parent().expect("path parent should exist now");
    if !parent.exists() {
      create_dir_all(parent)?;
    }
    let mut f = File::create(path)?;
    io::copy(&mut entry, &mut f)?;
    let perm = Permissions::from_mode(entry.header().mode());
    f.set_permissions(perm)?;
  }
  Ok(())
}

fn extract_deb(mut src: FlowMeter<impl Read + Seek>, dst: &Path) -> io::Result<()> {
  extract_ar(&mut src, dst)?;
  let mut pb = src.pb;
  let orig_len = pb.length();

  for x in ["control", "data"] {
    pb.reset();
    pb.set_message(format!("extracting {x}.tar.xz"));
    let control_path = dst.join(format!("{x}.tar.xz"));
    let f = File::open(&control_path)?;
    pb.set_length(f.metadata()?.len());
    let f = FlowMeter::new(f, pb);
    let mut ar = tar::Archive::new(XzDecoder::new(f));
    ar.unpack(dst.join(x))?;
    remove_file(control_path)?;
    pb = ar.into_inner().into_inner().pb;
  }

  if let Some(len) = orig_len {
    pb.set_length(len);
  }
  Ok(())
}

fn extract(
  kind: ArchiveKind,
  src: impl Read + Seek,
  dst: impl AsRef<Path>,
  pb: ProgressBar,
) -> io::Result<()> {
  use ArchiveKind::*;
  let src = FlowMeter::new(src, pb);
  match kind {
    Tar => tar::Archive::new(src).unpack(dst)?,
    TarGz => tar::Archive::new(GzDecoder::new(src)).unpack(dst)?,
    TarXz => tar::Archive::new(XzDecoder::new(src)).unpack(dst)?,
    TarBz2 => tar::Archive::new(BzDecoder::new(src)).unpack(dst)?,
    TarZst => tar::Archive::new(ZstDecoder::new(src)?).unpack(dst)?,
    Zip => ZipArchive::new(src)?.extract(dst)?,
    Ar => extract_ar(src, dst.as_ref())?,
    Deb => extract_deb(src, dst.as_ref())?,
  }
  Ok(())
}

async fn download(
  client: &Client,
  url: Url,
  mut dst: impl AsyncWrite + Unpin,
  pb: &ProgressBar,
) -> anyhow::Result<()> {
  let resp = client.get(url.clone()).send().await?.error_for_status()?;
  if let Some(len) = resp.content_length() {
    pb.set_length(len);
  }
  let mut stream = resp.bytes_stream();
  while let Some(bytes) = stream.try_next().await? {
    dst.write_all(&bytes).await?;
    pb.inc(bytes.len() as _);
  }
  Ok(())
}

// TODO: verify
async fn fetch_single_source_inner(
  source_dir: &Path,
  file: &SourceFile,
  client: Client,
  mp: MultiProgress,
) -> anyhow::Result<()> {
  let ar_kind = file
    .location
    .file_name()
    .and_then(ArchiveKind::from_file_name);

  let pb = mp.add(ProgressBar::new(1));
  let style = ProgressStyle::with_template(PB_STYLE_BYTES)
    .unwrap()
    .progress_chars("=> ");
  pb.set_style(style);
  pb.set_prefix(file.file_name().to_string());

  match &file.location {
    SourceLocation::Http(url) => {
      pb.set_message("downloading");
      let url = url.clone();
      if let Some((ar_kind, dir_name)) = ar_kind {
        let dir_name = file.rename.as_deref().unwrap_or(dir_name);
        let dst = source_dir.join(dir_name);
        let mut f = tempfile_async().await?;
        download(&client, url, &mut f, &pb).await?;
        let mut f = match f.try_into_std() {
          Ok(f) => f,
          Err(f) => f
            .try_clone()
            .await?
            .try_into_std()
            .expect("file should be ready once cloned"),
        };

        pb.reset();
        pb.set_message("extracting");
        let pb2 = pb.clone();
        asyncify(move || {
          f.rewind()?;
          extract(ar_kind, f, dst, pb2)
        })
        .await?;
      } else {
        let dst = source_dir.join(file.file_name());
        let mut f = AsyncFile::create(dst).await?;
        download(&client, url, &mut f, &pb).await?;
      }
    }
    SourceLocation::Local(path) => {
      pb.set_length(metadata(path).await?.len());
      if let Some((ar_kind, dir_name)) = ar_kind {
        let dir_name = file.rename.as_deref().unwrap_or(dir_name);
        let dst = source_dir.join(dir_name);
        pb.set_message("extracting");

        let pb2 = pb.clone();
        let path2 = path.clone();
        asyncify(move || extract(ar_kind, File::open(path2)?, dst, pb2)).await?;
      } else {
        let dst = source_dir.join(file.file_name());
        pb.set_message("copying");
        copy(path, dst).await?;
      }
    }
  }
  pb.finish_with_message("done");
  Ok(())
}

async fn fetch_single_source(
  source_dir: &Path,
  file: &SourceFile,
  client: Client,
  mp: MultiProgress,
) -> anyhow::Result<()> {
  fetch_single_source_inner(source_dir, file, client, mp)
    .map_err(|e| e.context(format!("failed to fetch '{}'", file.file_name())))
    .await
}

async fn fetch_source_inner(source_dir: &Path, files: &[SourceFile]) -> anyhow::Result<()> {
  if files.is_empty() {
    println!("No source specified, skipping");
  }

  const PARALLEL: usize = 5;
  let mut iter = files.iter();
  let mut pool = FuturesUnordered::new();
  let client = Client::new();
  let mp = MultiProgress::new();

  for file in iter.by_ref().take(PARALLEL) {
    pool.push(fetch_single_source(
      source_dir,
      file,
      client.clone(),
      mp.clone(),
    ));
  }

  while let Some(()) = pool.try_next().await? {
    if let Some(file) = iter.next() {
      pool.push(fetch_single_source(
        source_dir,
        file,
        client.clone(),
        mp.clone(),
      ));
    }
  }
  Ok(())
}

pub fn fetch_source(source_dir: &Path, files: &[SourceFile]) -> anyhow::Result<()> {
  let rt = RtBuilder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()?;
  rt.block_on(fetch_source_inner(source_dir, files))
}
