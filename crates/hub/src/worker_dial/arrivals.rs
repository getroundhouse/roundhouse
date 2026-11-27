use std::collections::HashMap;
use std::time::Duration;

use dialing::{Connection, RecvStream, SendStream};
use parking_lot::Mutex;
use protocol::model::InstanceId;
use tokio::sync::watch;

/// A worker the hub admitted, with the stream it said so on, which carries the worker protocol from then on.
#[expect(dead_code, reason = "the hub reaches a dialed worker through these in the next PR of #2478's stack")]
pub(crate) struct Dialed {
    pub(crate) instance_id: InstanceId,
    pub(crate) connection: Connection,
    pub(crate) send: SendStream,
    pub(crate) recv: RecvStream,
}

/// The admitted workers nobody has taken yet, one for each instance, the newest. One that dials again replaces what
/// it left, which is closed, so a worker always has one connection the hub can use.
#[derive(Default)]
pub(super) struct Arrivals {
    waiting: Mutex<HashMap<InstanceId, Dialed>>,
    /// Moves on each arrival, so whoever waits for one looks again.
    arrived: watch::Sender<u64>,
}

impl Arrivals {
    pub(super) fn arrive(&self, dialed: Dialed) {
        if let Some(older) = self.waiting.lock().insert(dialed.instance_id.clone(), dialed) {
            older.connection.close(0u32.into(), b"the worker dialed again");
        }
        self.arrived.send_modify(|count| *count = count.wrapping_add(1));
    }

    /// The instance's worker, once it has dialed, if it does within `within`.
    pub(super) async fn take(&self, instance_id: &InstanceId, within: Duration) -> Option<Dialed> {
        let mut arrived = self.arrived.subscribe();
        let waiting = async {
            loop {
                if let Some(dialed) = self.waiting.lock().remove(instance_id) {
                    return Some(dialed);
                }
                arrived.changed().await.ok()?;
            }
        };
        tokio::time::timeout(within, waiting).await.ok().flatten()
    }
}
