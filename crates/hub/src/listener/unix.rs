use std::io;
use std::path::Path;

use tokio::net::UnixListener;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

pub(crate) struct Listener(UnixListener);

impl Listener {
    /// Only this user can connect. The caller must hold the hub's lock, which means any socket already here is
    /// stale.
    pub(crate) fn bind(path: &Path) -> io::Result<Self> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        crate::private::restrict(path)?;
        Ok(Self(listener))
    }

    pub(crate) async fn accept(&mut self) -> io::Result<(OwnedReadHalf, OwnedWriteHalf)> {
        let (stream, _) = self.0.accept().await?;
        Ok(stream.into_split())
    }
}

/// Stops clients from finding a hub that's shutting down.
pub(crate) fn unbind(path: &Path) {
    let _ = std::fs::remove_file(path);
}
