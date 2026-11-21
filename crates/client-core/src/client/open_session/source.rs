//! Fetches and follows the hub's history for the view an open session requests.

use super::super::follow::Source;
use super::super::{Client, Result, Subscription, SubscriptionItem};
use super::View;
use crate::session_log::{By, Position, SessionLog};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use protocol::event::SessionEvent;
use protocol::model::{Place, Seq, SessionId, SubagentId};
use std::sync::Arc;

pub(super) struct SessionSource {
    pub(super) session_id: SessionId,
    /// The view asked for.
    pub(super) wanted: View,
    /// The view followed, which is every event when the hub can't read the agent's own apart.
    pub(super) view: View,
    /// How far the whole log went when the session was opened, for following it from a hub that can't read by
    /// place.
    pub(super) last_seq: Seq,
    /// The hub has said how far the events by place go.
    pub(super) placed: bool,
    /// The build of the hub its events were read from, as the hub's hello said.
    pub(super) read_from: String,
}

impl SessionSource {
    pub(super) fn subagent(&self) -> Option<&SubagentId> {
        self.view.subagent()
    }

    /// Reads the log again on a hub of another build than the one its events were read from, since a migration can
    /// have rewritten them, and catching up from one past the last would leave them as they were. It's read again as
    /// far as the hub's events go, which [`SessionSource::place`] has learned by then for a log by place, since the agent
    /// can have gone on while the hub restarted. Builds that no install or package stamped all say the same, so the log
    /// isn't read again between two of them.
    ///
    /// The build is what every hello already says, so the hub keeps nothing of its migrations for a client to compare.
    /// The cost is that every update reads each open chat's window again, once, though few updates rewrite an event.
    fn read_again_on_another_build(&mut self, client: &Client, held: &Mutex<Arc<SessionLog>>) {
        let build = client.hub_version();
        if self.read_from != build {
            build.clone_into(&mut self.read_from);
            change(held).read_again();
        }
    }

    /// Asks how far the events by place go, the first time on each connection. A hub that can't read the agent's own
    /// events apart is followed for every event instead, and one that can, after one that couldn't, by place again.
    pub(super) fn place(&mut self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<()> {
        if self.view == View::Whole || self.placed {
            return Ok(());
        }
        match client.session_read_places(&self.session_id, self.subagent(), Place::new(1), 1) {
            Ok((_, last)) => {
                let mut log = change(held);
                if log.by() == By::Seq {
                    log.read_by_place();
                }
                log.reaches(last);
            }
            Err(e) if e.is_unsupported() && self.view == View::Own => {
                self.view = View::Whole;
                // One already by `seq`, from such a hub before, carries on from where it got to.
                let mut log = change(held);
                if log.by() == By::Place {
                    log.read_whole(self.last_seq);
                }
            }
            Err(e) => return Err(e),
        }
        self.placed = true;
        Ok(())
    }
}

impl Source for SessionSource {
    type Held = Arc<SessionLog>;

    fn reconnected(&mut self) {
        self.view = self.wanted.clone();
        self.placed = false;
    }

    fn resume(&mut self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<Subscription> {
        self.place(client, held)?;
        self.read_again_on_another_build(client, held);
        // Events arrive in order with no gaps, so carrying on from one past the last misses nothing, whatever
        // was fetched meanwhile.
        let from = held.lock().received().next();
        let subscription = match &self.view {
            View::Whole => client.session_subscribe(&self.session_id, from.seq())?,
            View::Own | View::Subagent(_) => {
                client.session_subscribe_places(&self.session_id, self.subagent(), from.place())?
            }
        };
        change(held).retry_failed();
        Ok(subscription)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<Arc<SessionLog>>, item: SubscriptionItem) -> Result<bool> {
        let mut log = change(held);
        Ok(match item {
            SubscriptionItem::Event(event) => {
                log.apply_event(*event);
                true
            }
            SubscriptionItem::Tail(tail) => {
                log.apply_tail(tail);
                true
            }
            SubscriptionItem::Delta(delta) => {
                log.apply_delta(&delta);
                true
            }
            SubscriptionItem::Notice(_) | SubscriptionItem::Terminal(_) | SubscriptionItem::Github(_) => false,
        })
    }

    /// Fetches the pages around where the view is looking that aren't loaded, nearest first, telling readers of each as
    /// it's put in place, then each request waiting on an answer that none of them holds, then the session's prompts
    /// if they haven't been listed, once there's a connection to fetch them on.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<Arc<SessionLog>>, tell: &dyn Fn()) -> Result<bool> {
        let Some(client) = connection else { return Ok(false) };
        let pages = change(held).pages_to_fetch();
        let requests = change(held).requests_to_fetch();
        let fetched = !pages.is_empty() || !requests.is_empty();
        for range in pages {
            let start = range.start;
            match read_range(client, &self.session_id, &self.view, range) {
                Ok(events) => {
                    change(held).install_page(start, events);
                    tell();
                }
                Err(e) => {
                    change(held).fetch_failed();
                    return Err(e);
                }
            }
        }
        // By its `seq`, from the whole log, whichever events this view holds.
        for seq in requests {
            match client.session_read(&self.session_id, seq, 1, false) {
                Ok((events, _)) => change(held).install_request(seq, events.into_iter().next()),
                Err(e) => {
                    change(held).fetch_failed();
                    return Err(e);
                }
            }
        }
        let prompts = self.list_prompts(client, held)?;
        Ok(self.list_compactions(client, held)? || prompts || fetched)
    }
}

impl SessionSource {
    /// Asks for the session's prompts, once. It's only asked for here, with the subscription started, so it misses
    /// nothing: every prompt the subscription won't send was recorded before this. A hub too old to list them leaves
    /// only the prompts the log is given, and one that fails to is asked again on a new connection, while the chat goes
    /// on either way. A subagent's events hold no prompts, so a view of them asks for none.
    pub(super) fn list_prompts(&self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<bool> {
        if matches!(self.view, View::Subagent(_)) || !held.lock().wants_prompt_list() {
            return Ok(false);
        }
        match client.session_prompt_list(&self.session_id) {
            Ok(lines) => {
                change(held).list_prompts(lines);
                Ok(true)
            }
            Err(e) if e.is_unsupported() => {
                change(held).prompts_unlisted();
                Ok(false)
            }
            Err(e) => {
                change(held).prompt_list_failed();
                Err(e)
            }
        }
    }
}

impl SessionSource {
    /// Asks for the session's compactions, once, as [`SessionSource::list_prompts`] asks for its prompts, and for the
    /// same reasons. A subagent's compactions aren't in the chat, so a view of them asks for none.
    pub(super) fn list_compactions(&self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<bool> {
        if matches!(self.view, View::Subagent(_)) || !held.lock().wants_compaction_list() {
            return Ok(false);
        }
        match client.session_compaction_list(&self.session_id) {
            Ok(lines) => {
                change(held).list_compactions(lines);
                Ok(true)
            }
            Err(e) if e.is_unsupported() => {
                change(held).compactions_unlisted();
                Ok(false)
            }
            Err(e) => {
                change(held).compaction_list_failed();
                Err(e)
            }
        }
    }
}

/// The log, to change. A version a reader holds stays as it was, and the next one has the change.
fn change(held: &Mutex<Arc<SessionLog>>) -> MappedMutexGuard<'_, SessionLog> {
    MutexGuard::map(held.lock(), Arc::make_mut)
}

/// The events of `view` at these positions, which are its `seq`s in the whole log and its places in the rest. A size
/// limit can cut a read short, so this keeps going until the range is complete.
pub(super) fn read_range(
    client: &Client,
    session_id: &SessionId,
    view: &View,
    range: std::ops::Range<Position>,
) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();
    let mut from = range.start;
    while from < range.end {
        // As many as there are rows from here to the end. 0, where no event is, has no row, so it adds none.
        let rows = from.row().unwrap_or(0)..range.end.row().unwrap_or(0);
        let limit = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let (page, last) = match view {
            View::Whole => {
                let page = client.session_read(session_id, from.seq(), limit, false)?.0;
                let last = page.last().map(|e| Position::of_seq(e.seq));
                (page, last)
            }
            View::Own | View::Subagent(_) => {
                let page = client.session_read_places(session_id, view.subagent(), from.place(), limit)?.0;
                let last = page.last().map(|e| Position::of_place(e.place));
                (page, last)
            }
        };
        let Some(last) = last else { break };
        events.extend(page);
        from = last.next();
    }
    Ok(events)
}
