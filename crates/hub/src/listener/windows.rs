use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;

use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

/// A pipe has one instance for each client, so there's always one more than there are clients, waiting for the next.
pub(crate) struct Listener {
    name: OsString,
    waiting: NamedPipeServer,
}

impl Listener {
    /// Fails if anything already serves a pipe of this name, so a hub never shares its clients with another process.
    /// The pipe has the permissions Windows gives a pipe by default: this user, administrators and the system can
    /// read and write it, and anyone else can only read, so they can't send a request, and a hub only writes replies
    /// to requests.
    pub(crate) fn bind(path: &Path) -> io::Result<Self> {
        Ok(Self { name: path.into(), waiting: instance(path.as_os_str(), true)? })
    }

    pub(crate) async fn accept(&mut self) -> io::Result<(ReadHalf<NamedPipeServer>, WriteHalf<NamedPipeServer>)> {
        self.waiting.connect().await?;
        // Made before the connected one is handed over, so the pipe never goes away between clients. If this
        // fails, the connected one is still what's waiting, and the next call tries again.
        let next = instance(&self.name, false)?;
        Ok(tokio::io::split(std::mem::replace(&mut self.waiting, next)))
    }
}

/// One instance of the pipe. They all have to be made alike, or Windows refuses the ones after the first, and none
/// takes a client on another machine.
fn instance(name: &OsStr, first: bool) -> io::Result<NamedPipeServer> {
    ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).create(name)
}

/// Nothing to do. Once the listener goes, no instance is waiting, so a client finds the pipe busy until the hub's
/// last client has gone, and takes that to mean there's no hub.
pub(crate) fn unbind(_path: &Path) {}
