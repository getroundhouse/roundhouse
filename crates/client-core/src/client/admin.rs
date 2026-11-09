//! What a client can do with a hub of any version.

use std::path::Path;
use std::time::{Duration, Instant};

use protocol::client::HubStatus;

use super::{Client, ClientError, Result, hub_is_missing};

/// Longer than a hub waits for a stuck worker, which is 15 seconds.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// A connection that doesn't say hello, so it works across a real protocol break, when [`Client::connect`]
/// gets refused. All it can do is see the hub and stop it, which is what those requests never changing buys.
pub struct HubAdmin {
    client: Client,
}

impl HubAdmin {
    pub fn connect(socket: &Path) -> Result<Self> {
        Ok(Self { client: Client::open(socket)? })
    }

    pub fn status(&self) -> Result<HubStatus> {
        self.client.hub_status()
    }

    /// Stops the hub and its agents, and returns once it has exited. A hub holds this connection open
    /// until then, and only after it has let go of its lock, so a new one can start straight away.
    pub fn stop(self) -> Result<()> {
        self.client.hub_shutdown()?;
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !*self.client.inner.disconnected.lock() {
            if Instant::now() >= deadline {
                return Err(ClientError::Unexpected(format!("the hub didn't stop within {STOP_TIMEOUT:?}")));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}

/// Stops whichever hub is at this socket, and returns once it has exited. A hub that has already gone is what
/// was wanted, so that isn't an error.
pub fn stop_hub(socket: &Path) -> Result<()> {
    match HubAdmin::connect(socket) {
        Ok(hub) => hub.stop(),
        Err(e) if hub_is_missing(&e) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Stops the hub at this socket only if it's still the one with this `pid`, such as one someone was asked about, and
/// returns once it has exited. Another hub there, started since, is left alone, and one that has gone is what was
/// wanted, so neither is an error.
pub fn stop_hub_if_still(socket: &Path, pid: u32) -> Result<()> {
    let hub = match HubAdmin::connect(socket) {
        Ok(hub) => hub,
        Err(e) if hub_is_missing(&e) => return Ok(()),
        Err(e) => return Err(e),
    };
    if hub.status()?.pid == pid { hub.stop() } else { Ok(()) }
}
