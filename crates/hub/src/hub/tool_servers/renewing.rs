use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::ToolServerId;
use tokio::sync::watch;

use crate::store::later_ms;
use crate::tool_servers::{Due, Held};

/// The renewals of tool servers' sign-ins under way, and those that failed, as may pass. In memory, since a renewal under
/// way ends with the hub, and what failed under one is worth trying again as the next starts.
#[derive(Clone, Default)]
pub(super) struct Renewing {
    renewing: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// Told of nothing, and gone once the renewal has ended, by server.
    under_way: HashMap<ToolServerId, watch::Receiver<()>>,
    failed: HashMap<ToolServerId, Failed>,
    /// Whether the hub is stopping, so none starts.
    stopped: bool,
}

struct Failed {
    of: Held,
    /// One after another.
    times: u32,
    at_ms: i64,
}

/// How long a renewal that failed once waits to be tried again, which each check after it's due does.
const FIRST_WAIT: Duration = Duration::from_mins(1);

/// The most one waits, which keeps an authorization server that's down for long from being asked much, and doesn't leave
/// agents without it for long once it's back.
const LONGEST_WAIT: Duration = Duration::from_mins(10);

/// How long a renewal that failed `times` in a row waits to be tried again.
pub(super) fn wait_after(times: u32) -> Duration {
    let doubled = 2_u32.checked_pow(times.saturating_sub(1)).unwrap_or(u32::MAX);
    FIRST_WAIT.checked_mul(doubled).unwrap_or(LONGEST_WAIT).min(LONGEST_WAIT)
}

/// A renewal under way, for as long as it's held.
pub(super) struct UnderWay {
    renewing: Renewing,
    server: ToolServerId,
    _ended: watch::Sender<()>,
}

impl Renewing {
    /// Whether `due` has waited as long as it does after it failed, or hasn't failed.
    pub(super) fn has_waited(&self, due: &Due, now_ms: i64) -> bool {
        let renewing = self.renewing.lock();
        let failed = renewing.failed.get(&due.server).filter(|failed| failed.of == due.of);
        failed.is_none_or(|failed| now_ms >= later_ms(failed.at_ms, wait_after(failed.times)))
    }

    /// Has a renewal of the sign-in to `server` be under way. `None` when one is, so there's never a second, which would
    /// send the authorization server a refresh token it has replaced, and when the hub is stopping.
    pub(super) fn begin(&self, server: &ToolServerId) -> Option<UnderWay> {
        let mut renewing = self.renewing.lock();
        if renewing.stopped || renewing.under_way.contains_key(server) {
            return None;
        }
        let (ended, ends) = watch::channel(());
        renewing.under_way.insert(server.clone(), ends);
        Some(UnderWay { renewing: self.clone(), server: server.clone(), _ended: ended })
    }

    /// `due` failed at `now_ms`, so it waits longer than the last time.
    pub(super) fn failed(&self, due: &Due, now_ms: i64) {
        let mut renewing = self.renewing.lock();
        let before = renewing.failed.remove(&due.server).filter(|failed| failed.of == due.of);
        let times = before.map_or(0, |failed| failed.times).saturating_add(1);
        renewing.failed.insert(due.server.clone(), Failed { of: due.of.clone(), times, at_ms: now_ms });
    }

    /// The server's sign-in was renewed, or has gone.
    pub(super) fn settled(&self, server: &ToolServerId) {
        self.renewing.lock().failed.remove(server);
    }

    /// Starts none after, and waits for each under way to end.
    pub(super) async fn stop(&self) {
        self.renewing.lock().stopped = true;
        self.ended().await;
    }

    /// Waits for each under way to end.
    pub(super) async fn ended(&self) {
        let under_way: Vec<_> = self.renewing.lock().under_way.values().cloned().collect();
        for mut ends in under_way {
            // An error says it has, since nothing is ever told.
            let _ended = ends.changed().await;
        }
    }
}

impl Drop for UnderWay {
    fn drop(&mut self) {
        self.renewing.renewing.lock().under_way.remove(&self.server);
    }
}
