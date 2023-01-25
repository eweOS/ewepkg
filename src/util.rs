use console::style;
use tempfile::tempfile;
use tokio::fs::File;
use tokio::io;
use tokio::task::spawn_blocking;

// Taken from Tokio
pub async fn asyncify<F, T>(f: F) -> io::Result<T>
where
  F: FnOnce() -> io::Result<T> + Send + 'static,
  T: Send + 'static,
{
  match spawn_blocking(f).await {
    Ok(res) => res,
    Err(_) => Err(io::Error::new(
      io::ErrorKind::Other,
      "background task failed",
    )),
  }
}

pub async fn tempfile_async() -> io::Result<File> {
  let std_file = asyncify(tempfile).await?;
  Ok(File::from_std(std_file))
}

pub fn segment_info(msg: &str) {
  println!("{} {}", style("::").green().bold(), style(msg).bold())
}
