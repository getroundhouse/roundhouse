use std::time::Duration;

use tracing::warn;

use crate::child_process::ChildProcess;

/// What carries a worker's stream: the connection a worker dialed the hub on, which the hub holds until the worker has
/// gone, or the command the hub runs to reach one of the local provider's workers that doesn't dial.
pub(crate) enum Carrier {
    Command(ChildProcess),
    Dialed(dialing::Connection),
}

/// What the hub closes a dialed worker's connection with, as QUIC's application error code, once it's done with it.
const ENDED: u32 = 0;

impl Carrier {
    /// Ends it at once, without waiting, which leaves its provider to end the worker.
    pub(crate) fn kill(&self) {
        match self {
            Self::Command(process) => process.kill(),
            Self::Dialed(connection) => connection.close(ENDED.into(), b"the hub ended the worker's connection"),
        }
    }

    /// Waits up to `grace` for it to end, as the worker's command exits, or a dialed worker closes its connection, once
    /// the worker has stopped, and ends it if it hasn't.
    pub(crate) async fn end(&self, grace: Duration) {
        match self {
            Self::Command(process) => process.end(grace).await,
            Self::Dialed(connection) => {
                if tokio::time::timeout(grace, connection.closed()).await.is_err() {
                    warn!(name: "dialed.not_closed", "A worker that dialed the hub didn't close its connection in time, so the hub closes it");
                    self.kill();
                }
            }
        }
    }
}
