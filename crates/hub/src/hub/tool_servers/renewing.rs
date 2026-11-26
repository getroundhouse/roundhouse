use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::ToolServerId;
use tokio::sync::watch;

use crate::store::later_ms;
use crate::tool_servers::{Due, Held};

/// The renewals of tool servers' sign-ins under way, and how the last of each that ended went. In memory, since a renewal
/// under way ends with the hub, and what failed under one is worth trying again as the next starts.
#[derive(Clone, Default)]
pub(super) struct Renewing {
    renewing: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// Told of nothing, and gone once the renewal has ended, by server.
    under_way: HashMap<ToolServerId, watch::Receiver<()>>,
    last: HashMap<ToolServerId, Last>,
    /// Whether the hub is stopping, so none starts.
    stopped: bool,
}

struct Last {
    /// The tokens it left.
    left: Held,
    /// How many have failed one after another, as may pass.
    failed: u32,
    at_ms: i64,
}

/// How long after a renewal ends another isn't made for a token a server refused. A server that refuses one it was given
/// a moment ago refuses the next too, and an authorization server that couldn't be asked a moment ago still can't, so an
/// agent that goes on calling doesn't have one asked at each call. *Unconfirmed.*
const ASKED_APART: Duration = Duration::from_mins(1);

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

/// The end of a renewal under way.
pub(super) struct Ends(watch::Receiver<()>);

impl Ends {
    pub(super) async fn ended(mut self) {
        // An error says it has, since nothing is ever told.
        let _ended = self.0.changed().await;
    }
}

/// What there is to wait for, for a token a server refused.
pub(super) enum Joined {
    /// A renewal to make, now under way.
    Begun(UnderWay, Ends),
    /// The one that was under way.
    UnderWay(Ends),
    /// None, as `ASKED_APART` says, or since the hub is stopping.
    Nothing,
}

impl Renewing {
    /// Whether `due` has waited as long as it does after it failed, or hasn't failed.
    pub(super) fn has_waited(&self, due: &Due, now_ms: i64) -> bool {
        let renewing = self.renewing.lock();
        let failed = renewing.last.get(&due.server).filter(|last| last.left == due.of && last.failed > 0);
        failed.is_none_or(|last| now_ms >= later_ms(last.at_ms, wait_after(last.failed)))
    }

    /// Has a renewal of the sign-in to `server` be under way. `None` when one is, so there's never a second, which would
    /// send the authorization server a refresh token it has replaced, and when the hub is stopping.
    pub(super) fn begin(&self, server: &ToolServerId) -> Option<UnderWay> {
        self.begin_in(&mut self.renewing.lock(), server).map(|(under_way, _)| under_way)
    }

    fn begin_in(&self, renewing: &mut State, server: &ToolServerId) -> Option<(UnderWay, Ends)> {
        if renewing.stopped || renewing.under_way.contains_key(server) {
            return None;
        }
        let (ended, ends) = watch::channel(());
        renewing.under_way.insert(server.clone(), ends.clone());
        Some((UnderWay { renewing: self.clone(), server: server.clone(), _ended: ended }, Ends(ends)))
    }

    /// The renewal to wait for of `due`, whose token its server has refused, at `now_ms`.
    pub(super) fn join(&self, due: &Due, now_ms: i64) -> Joined {
        let mut renewing = self.renewing.lock();
        if let Some(ends) = renewing.under_way.get(&due.server) {
            return Joined::UnderWay(Ends(ends.clone()));
        }
        let last = renewing.last.get(&due.server).filter(|last| last.left == due.of);
        if last.is_some_and(|last| now_ms < later_ms(last.at_ms, ASKED_APART)) {
            return Joined::Nothing;
        }
        self.begin_in(&mut renewing, &due.server)
            .map_or(Joined::Nothing, |(under_way, ends)| Joined::Begun(under_way, ends))
    }

    /// `due` failed at `now_ms`, so it waits longer than the last time.
    pub(super) fn failed(&self, due: &Due, now_ms: i64) {
        let mut renewing = self.renewing.lock();
        let before = renewing.last.remove(&due.server).filter(|last| last.left == due.of);
        let failed = before.map_or(0, |last| last.failed).saturating_add(1);
        renewing.last.insert(due.server.clone(), Last { left: due.of.clone(), failed, at_ms: now_ms });
    }

    /// `due` was renewed at `now_ms`.
    pub(super) fn renewed(&self, due: &Due, now_ms: i64) {
        let last = Last { left: due.of.renewed(), failed: 0, at_ms: now_ms };
        self.renewing.lock().last.insert(due.server.clone(), last);
    }

    /// The sign-in to the server `id` is gone, or another.
    pub(super) fn gone(&self, id: &ToolServerId) {
        self.renewing.lock().last.remove(id);
    }

    /// Starts none after, and waits for each under way to end.
    pub(super) async fn stop(&self) {
        self.renewing.lock().stopped = true;
        self.ended().await;
    }

    /// Waits for each under way to end.
    pub(super) async fn ended(&self) {
        let under_way: Vec<_> = self.renewing.lock().under_way.values().cloned().collect();
        for ends in under_way {
            Ends(ends).ended().await;
        }
    }
}

impl Drop for UnderWay {
    fn drop(&mut self) {
        self.renewing.renewing.lock().under_way.remove(&self.server);
    }
}
