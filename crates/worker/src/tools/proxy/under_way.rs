use super::refusals::{Refusals, Refused};
use parking_lot::Mutex;
use protocol::model::ToolServerId;
use std::collections::HashMap;
use std::sync::Arc;

/// How many requests each of the hub's servers can have under way at once, from all of this worker's agents together:
/// more than each agent's one call at a time, with room for several to call it at once, and few enough that a server
/// that never answers holds no more than this many of the connections the worker takes. With the four agents and ten
/// servers [`MAX_CONNECTIONS`](super::super::server::MAX_CONNECTIONS) is sized for, each with a call and a stream of
/// events open to each server, and one server stuck at this, that's 92 of its 96. *Unconfirmed.*
pub(super) const MOST_UNDER_WAY: usize = 8;

/// The requests under way to each server, by its ID, so a set of the servers, which makes each again, counts on.
#[derive(Default)]
pub(super) struct UnderWay(Arc<Mutex<HashMap<ToolServerId, Count>>>);

#[derive(Default)]
struct Count {
    under_way: usize,
    /// Those refused since the last that ended.
    refusals: Refusals,
}

/// A request to one server, counted until it's dropped.
pub(super) struct Counted {
    counts: Arc<Mutex<HashMap<ToolServerId, Count>>>,
    id: ToolServerId,
}

impl UnderWay {
    /// A request to the server `id`, unless it has as many under way as it may.
    pub(super) fn counted(&self, id: &ToolServerId) -> Result<Counted, Refused> {
        let mut counts = self.0.lock();
        let count = counts.entry(id.clone()).or_default();
        if count.under_way >= MOST_UNDER_WAY {
            return Err(count.refusals.refused());
        }
        count.under_way += 1;
        Ok(Counted { counts: self.0.clone(), id: id.clone() })
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        let mut counts = self.counts.lock();
        if let Some(count) = counts.get_mut(&self.id) {
            count.under_way -= 1;
            count.refusals = Refusals::default();
            if count.under_way == 0 {
                counts.remove(&self.id);
            }
        }
    }
}
