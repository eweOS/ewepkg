use tempfile::tempfile;
use tokio::fs::File;
use tokio::io;
use tokio::task::spawn_blocking;

pub const PB_STYLE: &str =
  "{wide_msg}  {pos:>10} {len:>10} [{bar:20.blue}] {percent:>3}%  {prefix:<11!} ";

pub const PB_STYLE_BYTES: &str =
  "{wide_msg}  {bytes:>10} {total_bytes:>10} [{bar:20.blue}] {percent:>3}%  {prefix:<11!} ";

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

#[macro_export]
macro_rules! segment_info {
  ($msg:expr) => {
    println!(
      "{} {}",
      console::style("::").green().bold(),
      console::style($msg).bold()
    );
  };
  ($msg:expr, $($arg:tt)*) => {
    print!("{} {} ",
      console::style("::").green().bold(),
      console::style($msg).bold()
    );
    println!($($arg)*);
  };
}
