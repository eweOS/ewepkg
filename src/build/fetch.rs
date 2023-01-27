use crate::source::{SourceFile, SourceLocation};
use crate::util::{asyncify, tempfile_async};
use flate2::read::GzDecoder;
use futures::stream::FuturesUnordered;
use futures::{TryFutureExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{Client, Url};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use tokio::fs::{copy, metadata, File as AsyncFile};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::runtime::Runtime;
use xz2::read::XzDecoder;

const PB_STYLE: &str =
  "{prefix:<30!}  {bytes:>10} {total_bytes:>10} [{wide_bar:.blue}] {percent:>3}%  {msg:12}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
  Tar,
  TarGz,
  TarXz,
  Zip,
  Ar,
}

impl ArchiveKind {
  fn from_file_name(name: &str) -> Option<(Self, &str)> {
    let len = name.len();
    if name.ends_with(".tar") {
      Some((Self::Tar, &name[..len - 4]))
    } else if name.ends_with(".tar.gz") {
      Some((Self::TarGz, &name[..len - 7]))
    } else if name.ends_with(".tar.xz") {
      Some((Self::TarXz, &name[..len - 7]))
    } else if name.ends_with(".zip") {
      Some((Self::Zip, &name[..len - 4]))
    } else if name.ends_with(".deb") {
      Some((Self::Ar, &name[..len - 4]))
    } else {
      None
    }
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

fn extract(
  kind: ArchiveKind,
  src: impl Read,
  dst: impl AsRef<Path>,
  pb: ProgressBar,
) -> io::Result<()> {
  use ArchiveKind::*;
  let src = FlowMeter::new(src, pb);
  match kind {
    Tar => tar::Archive::new(src).unpack(dst)?,
    TarGz => tar::Archive::new(GzDecoder::new(src)).unpack(dst)?,
    TarXz => tar::Archive::new(XzDecoder::new(src)).unpack(dst)?,
    Zip => todo!(),
    Ar => todo!(),
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
  let style = ProgressStyle::with_template(PB_STYLE)
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
        let mut f = f
          .try_into_std()
          .expect("all async file operations should be done");

        pb.set_position(0);
        pb.set_message("extracting");
        let pb2 = pb.clone();
        asyncify(move || {
          f.seek(SeekFrom::Start(0))?;
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
  // m.clear()?;
  Ok(())
}

pub fn fetch_source(source_dir: &Path, files: &[SourceFile]) -> anyhow::Result<()> {
  let rt = Runtime::new()?;
  rt.block_on(fetch_source_inner(source_dir, files))
}
