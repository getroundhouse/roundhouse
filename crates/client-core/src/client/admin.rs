//! What a client can do with a hub of any version.

use std::path::Path;
use std::time::{Duration, Instant};

use protocol::client::{ClientRequest, ErrorCode, HubStatus};

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
        wait_for_exit(&self.client)
    }
}

/// Returns once the hub this client asked to stop has exited, which closes the connection.
fn wait_for_exit(client: &Client) -> Result<()> {
    if exited_within(client, STOP_TIMEOUT) {
        Ok(())
    } else {
        Err(ClientError::Unexpected(format!("the hub didn't stop within {STOP_TIMEOUT:?}")))
    }
}

/// Whether the hub this client asked to go has exited within `wait`, which closes the connection.
fn exited_within(client: &Client, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while !*client.inner.disconnected.lock() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// What came of asking a hub to restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    /// It has exited, so the next client to connect starts the next hub.
    Exited,
    /// It's still finishing what it waits for first, such as a setup script, and goes by itself once that's done,
    /// which closes this connection. Meanwhile it carries on as before.
    Finishing,
}

/// Restarts the hub this client is connected to, which stops nothing: its workers carry on with their agents, for the
/// next hub to connect to again. Only a hub whose status says [`HubStatus::keeps_workers`] knows how. It first waits for
/// a setup script that's running, so with one running this returns at once, and otherwise once the hub has exited,
/// unless something still holds it up after as long as a hub gets to stop, such as another client's request.
pub fn restart_hub(client: &Client) -> Result<Restart> {
    let setups = client.hub_status()?.running_setups;
    match client.expect_ok(ClientRequest::HubRestart) {
        // The hub did what was asked before its reply got out.
        Err(e) if hub_is_missing(&e) => {}
        other => other?,
    }
    let exited = setups == 0 && exited_within(client, STOP_TIMEOUT);
    Ok(if exited { Restart::Exited } else { Restart::Finishing })
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

/// What came of asking a hub to stop if it's idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleStop {
    /// It was idle, and has exited.
    Stopped,
    /// Something was running on it or using it after all, so it carries on, and so does this connection.
    Busy,
}

/// Stops the hub this client is connected to if nothing would notice, and returns once it has exited, or says it's
/// busy. The hub decides as it stops, so a prompt sent after the client last saw it idle isn't cut off. Asked on the
/// client's own connection, since the hub counts any other as someone who'd notice.
///
/// A hub from before it could decide is stopped as [`stop_hub_if_still`] does, if it's still the one with this `pid`,
/// on the caller's word that it was idle, which is all there was to go on before.
pub fn stop_hub_if_idle(client: &Client, socket: &Path, pid: u32) -> Result<IdleStop> {
    match client.hub_shutdown_if_idle() {
        Ok(()) => wait_for_exit(client).map(|()| IdleStop::Stopped),
        Err(ClientError::Api(e)) if e.code == ErrorCode::Conflict => Ok(IdleStop::Busy),
        Err(e) if e.is_unsupported() => stop_hub_if_still(socket, pid).map(|()| IdleStop::Stopped),
        Err(e) => Err(e),
    }
}
