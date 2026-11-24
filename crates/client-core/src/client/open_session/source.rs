//! Fetches and follows the hub's history for the view an open session requests.

use super::super::follow::{Reach, Source};
use super::super::{Client, ClientError, Result, Subscription, SubscriptionItem};
use super::View;
use crate::records::HubRecords;
use crate::session_log::{Position, SessionLog};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, LogFrom};
use protocol::event::SessionEvent;
use protocol::model::{Place, SessionId, SubagentId};
use std::sync::Arc;

pub(super) struct SessionSource {
    pub(super) session_id: SessionId,
    pub(super) view: View,
    /// The hub has said how far the events go.
    pub(super) placed: bool,
    /// The build of the hub its events were read from, as the hub's hello said.
    pub(super) read_from: String,
    /// The copy of the hub's records the session's own records go into, as its subscription sends them, which wants
    /// them from when the session is opened until it ends.
    pub(super) records: Reach<HubRecords>,
}

impl SessionSource {
    pub(super) fn subagent(&self) -> Option<&SubagentId> {
        self.view.subagent()
    }

    /// Reads the log again on a hub of another build than the one its events were read from, since a migration can
    /// have rewritten them, and catching up from one past the last would leave them as they were. It's read again as
    /// far as the hub's events go, which [`SessionSource::place`] has learned by then, since the agent can have gone on
    /// while the hub restarted. Builds that no install or package stamped all say the same, so the log isn't read again
    /// between two of them.
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

    /// Asks how far the events go, the first time on each connection.
    pub(super) fn place(&mut self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<()> {
        if self.placed {
            return Ok(());
        }
        let (_, last) = client.session_read_places(&self.session_id, self.subagent(), Place::new(1), 1)?;
        change(held).reaches(last);
        self.placed = true;
        Ok(())
    }
}

impl Source for SessionSource {
    type Held = Arc<SessionLog>;

    fn reconnected(&mut self) {
        self.placed = false;
    }

    fn resume(&mut self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<Subscription> {
        self.place(client, held)?;
        self.read_again_on_another_build(client, held);
        // Events arrive in order with no gaps, so carrying on from one past the last misses nothing, whatever
        // was fetched meanwhile.
        let place = held.lock().received().next().place();
        let from = LogFrom::Place { subagent: self.subagent().cloned(), place };
        let subscription = client.follow(Follow::Session { id: self.session_id.clone(), from: Some(from) })?;
        change(held).retry_failed();
        Ok(subscription)
    }

    /// Takes in what the session's subscription sends: its log, into what's held, and its own records, into the copy of
    /// the hub's records.
    fn apply(&mut self, _: &Client, held: &Mutex<Arc<SessionLog>>, item: SubscriptionItem) -> Result<bool> {
        let records = |change: &dyn Fn(&mut HubRecords) -> bool| {
            self.records.change(change);
        };
        let mut log = change(held);
        Ok(match item {
            SubscriptionItem::Listed(listed) => {
                if let Listed::Session(listed) = *listed {
                    records(&|records| records.apply_session_list(listed.clone()));
                }
                false
            }
            SubscriptionItem::Notice(notice) => {
                records(&|records| records.apply_notice((*notice).clone()));
                false
            }
            SubscriptionItem::Event(event) => {
                log.apply_event(placed(*event)?);
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
            SubscriptionItem::Terminal(_) | SubscriptionItem::TerminalPicture(_) => false,
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
    /// nothing: every prompt the subscription won't send was recorded before this. A hub that fails to list them is
    /// asked again on a new connection, while the chat goes on either way. A subagent's events hold no prompts, so a
    /// view of them asks for none.
    pub(super) fn list_prompts(&self, client: &Client, held: &Mutex<Arc<SessionLog>>) -> Result<bool> {
        if matches!(self.view, View::Subagent(_)) || !held.lock().wants_prompt_list() {
            return Ok(false);
        }
        match client.session_prompt_list(&self.session_id) {
            Ok(lines) => {
                change(held).list_prompts(lines);
                Ok(true)
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

/// The events of `view` at these positions. A size limit can cut a read short, so this keeps going until the range is
/// complete.
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
        let page = client.session_read_places(session_id, view.subagent(), from.place(), limit)?.0;
        let page = page.into_iter().map(placed).collect::<Result<Vec<_>>>()?;
        let Some(last) = page.last().map(Position::of) else { break };
        events.extend(page);
        from = last.next();
    }
    Ok(events)
}

/// `event`, which the hub sent with its place among the events from its agent or subagent, as it records every event
/// with one. A log is held by place, so one with none is refused, rather than left out without a word.
fn placed(event: SessionEvent) -> Result<SessionEvent> {
    if event.place == Place::default() {
        return Err(ClientError::Unexpected(format!("event {} of {} has no place", event.seq, event.session_id)));
    }
    Ok(event)
}

/// Lets go of the session's own records once nothing it follows of the session is left, as a view that closes its chat
/// does, since they reach a client only while it follows the session.
impl Drop for SessionSource {
    fn drop(&mut self) {
        self.records.change(|records| records.forget_session(&self.session_id));
    }
}
