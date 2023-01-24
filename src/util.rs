use futures::TryFutureExt;
use std::path::Path;
use tokio::fs;
use tokio::io;

pub async fn mv_async(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
  let (from, to) = (from.as_ref(), to.as_ref());
  fs::rename(from, to)
    .or_else(|_e| fs::copy(from, to).map_ok(|_| ()))
    .await
}
