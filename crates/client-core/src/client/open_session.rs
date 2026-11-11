//! One session's log, kept current so a view only reads it: a window of its history and its live tail. A view
//! follows every event, or only the agent's own, or one subagent's.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::attachment::Attachment;
use protocol::event::SessionEvent;
use protocol::find::Needle;
use protocol::model::{ClientMessageId, Place, Seq, Session, SessionId, ToolUseId};

use super::follow::{Follow, FollowError, Source};
use super::{Client, Result, Subscription, SubscriptionItem};
use crate::session_log::{By, Changes, PendingPrompt, SessionLog};
use search::Searches;
pub use search::{Search, Searched, Stretch};

mod search;

/// Which of a session's events a view follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// Every event, by `seq`.
    Whole,
    /// The agent's own events, by place. A hub from before places can't read them apart, so from one this is every
    /// event, by `seq`, which [`SessionLog::by`] says.
    Own,
    /// The events of the subagent the call `call` started, by place.
    Subagent(ToolUseId),
}

impl View {
    /// The subagent whose events it follows, or `None` for the agent's own, or every event.
    fn subagent(&self) -> Option<&ToolUseId> {
        match self {
            View::Subagent(call) => Some(call),
            View::Whole | View::Own => None,
        }
    }
}

/// A session someone has open, kept current until every clone of this has gone. It fetches the history a
/// view is looking at, and carries on from a new connection given to [`OpenSession::reconnect`].
#[derive(Clone)]
pub struct OpenSession {
    follow: Follow<SessionLog>,
    session_id: SessionId,
    /// The view asked for, which is what a search looks through.
    view: View,
    searches: Arc<Searches>,
}

impl OpenSession {
    /// Starts following every event, on a thread of its own, so this doesn't wait for the hub. Once it has subscribed,
    /// it asks for the session's prompts.
    pub fn start(client: &Client, session: &Session) -> OpenSession {
        OpenSession::start_view(client, session, View::Whole)
    }

    /// Starts following the events `view` takes, on a thread of its own. Once it has subscribed, it asks for the
    /// session's prompts, unless it follows a subagent, whose events hold none.
    pub fn start_view(client: &Client, session: &Session, view: View) -> OpenSession {
        // One by place doesn't know how far it goes until the hub says.
        let log = if view == View::Whole { SessionLog::new(session.last_seq) } else { SessionLog::by_place(0) };
        let source = SessionSource {
            session_id: session.id.clone(),
            wanted: view.clone(),
            view: view.clone(),
            last_seq: session.last_seq,
            placed: false,
        };
        let follow = Follow::start(client, log, source);
        follow.wake();
        OpenSession { follow, session_id: session.id.clone(), view, searches: Arc::default() }
    }

    /// Says that something changed. It holds one signal at a time, so changes a view hasn't looked at yet
    /// gather into one, and a slow view never falls behind. Each signal goes to one receiver, so one view
    /// listens.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    pub fn read<T>(&self, read: impl FnOnce(&SessionLog) -> T) -> T {
        self.follow.read(read)
    }

    /// What changed since a view last looked.
    pub fn take_changes(&self) -> Changes {
        self.follow.quietly(SessionLog::take_changes)
    }

    /// Fetches what a view looking at this row needs.
    pub fn want(&self, row: usize) {
        self.follow.quietly(|log| log.focus_on(row));
        self.follow.wake();
    }

    /// Shows a prompt on its way, with what it carries.
    pub fn add_pending(&self, client_msg_id: ClientMessageId, text: String, attachments: Vec<Attachment>) {
        self.follow.update(|log| log.add_pending(client_msg_id, text, attachments));
    }

    /// Takes back a prompt that couldn't be sent.
    pub fn remove_pending(&self, client_msg_id: &ClientMessageId) -> Option<PendingPrompt> {
        self.follow.update(|log| log.remove_pending(client_msg_id))
    }

    /// Carries on on a new connection, from the last event it received.
    pub fn reconnect(&self, client: &Client) {
        self.searches.reconnected();
        self.follow.reconnect(client);
    }

    /// Looks through all of what the view follows for `needle`, which the log holds only a window of, on a thread of
    /// its own, and says when there's more through [`OpenSession::changed`]. The hub says which events hold it, by
    /// place, and the search reads each with what's within `around` places of it, a stretch at a time, for a view to
    /// count what the rows it doesn't hold draw. Events the log holds, and those it's given from now on, a view looks
    /// through itself. A log by `seq` is from a hub too old to read by place, and so to find. The search stops once it
    /// has gone, and `client` is the connection it asks on.
    pub fn search(&self, client: &Client, needle: Needle, around: u64) -> Search {
        let (every, by) = self.follow.read(|log| (log.holds_every_event(), log.by()));
        if every {
            return Search::finished(Searched::Done);
        }
        if needle.query().trim().chars().count() < Search::SHORTEST {
            return Search::finished(Searched::TooShort);
        }
        if by == By::Seq {
            return Search::finished(Searched::CantFind);
        }
        let asking = search::Asking { session_id: self.session_id.clone(), view: self.view.clone(), needle, around };
        search::Job::start(client, asking, &self.searches, self.follow.teller())
    }

    pub fn take_errors(&self) -> Vec<FollowError> {
        self.follow.take_errors()
    }
}

struct SessionSource {
    session_id: SessionId,
    /// The view asked for.
    wanted: View,
    /// The view followed, which is every event when the hub can't read the agent's own apart.
    view: View,
    /// How far the whole log went when the session was opened, for following it from a hub that can't read by
    /// place.
    last_seq: Seq,
    /// The hub has said how far the events by place go.
    placed: bool,
}

impl SessionSource {
    fn subagent(&self) -> Option<&ToolUseId> {
        self.view.subagent()
    }

    /// Asks how far the events by place go, the first time on each connection. A hub that can't read the agent's own
    /// events apart is followed for every event instead, and one that can, after one that couldn't, by place again.
    fn place(&mut self, client: &Client, held: &Mutex<SessionLog>) -> Result<()> {
        if self.view == View::Whole || self.placed {
            return Ok(());
        }
        match client.session_read_places(&self.session_id, self.subagent(), Place::new(1), 1) {
            Ok((_, last)) => {
                let mut log = held.lock();
                if log.by() == By::Seq {
                    log.read_by_place();
                }
                log.reaches(last.get());
            }
            Err(e) if e.is_unsupported() && self.view == View::Own => {
                self.view = View::Whole;
                // One already by `seq`, from such a hub before, carries on from where it got to.
                let mut log = held.lock();
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
    type Held = SessionLog;

    fn reconnected(&mut self) {
        self.view = self.wanted.clone();
        self.placed = false;
    }

    fn resume(&mut self, client: &Client, held: &Mutex<SessionLog>) -> Result<Subscription> {
        self.place(client, held)?;
        // Events arrive in order with no gaps, so carrying on from one past the last misses nothing, whatever
        // was fetched meanwhile.
        let from = held.lock().received() + 1;
        let subscription = match &self.view {
            View::Whole => client.session_subscribe(&self.session_id, from)?,
            View::Own | View::Subagent(_) => {
                client.session_subscribe_places(&self.session_id, self.subagent(), Place::new(from))?
            }
        };
        held.lock().retry_failed();
        Ok(subscription)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<SessionLog>, item: SubscriptionItem) -> Result<bool> {
        let mut log = held.lock();
        Ok(match item {
            SubscriptionItem::Event(event) => {
                log.apply_event(event);
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
            SubscriptionItem::Notice(_) => false,
        })
    }

    /// Fetches the pages around where the view is looking that aren't loaded, then the session's prompts if they
    /// haven't been listed.
    fn asked(&mut self, client: &Client, held: &Mutex<SessionLog>) -> Result<bool> {
        let pages = held.lock().pages_to_fetch();
        let fetched = !pages.is_empty();
        for range in pages {
            let start = range.start;
            match read_range(client, &self.session_id, &self.view, range) {
                Ok(events) => held.lock().install_page(start, events),
                Err(e) => {
                    held.lock().fetch_failed();
                    return Err(e);
                }
            }
        }
        Ok(self.list_prompts(client, held)? || fetched)
    }
}

impl SessionSource {
    /// Asks for the session's prompts, once. It's only asked for here, with the subscription started, so it misses
    /// nothing: every prompt the subscription won't send was recorded before this. A hub too old to list them leaves
    /// only the prompts the log is given, and one that fails to is asked again on a new connection, while the chat goes
    /// on either way. A subagent's events hold no prompts, so a view of them asks for none.
    fn list_prompts(&self, client: &Client, held: &Mutex<SessionLog>) -> Result<bool> {
        if matches!(self.view, View::Subagent(_)) || !held.lock().wants_prompt_list() {
            return Ok(false);
        }
        match client.session_prompt_list(&self.session_id) {
            Ok(lines) => {
                held.lock().list_prompts(lines);
                Ok(true)
            }
            Err(e) if e.is_unsupported() => {
                held.lock().prompts_unlisted();
                Ok(false)
            }
            Err(e) => {
                held.lock().prompt_list_failed();
                Err(e)
            }
        }
    }
}

/// The events of `view` at these positions. A size limit can cut a read short, so this keeps going until the range is
/// complete.
fn read_range(
    client: &Client,
    session_id: &SessionId,
    view: &View,
    range: std::ops::Range<u64>,
) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();
    let mut from = range.start;
    while from < range.end {
        let limit = u32::try_from(range.end - from).unwrap_or(u32::MAX);
        let (page, last) = match view {
            View::Whole => {
                let page = client.session_read(session_id, from, limit, false)?.0;
                let last = page.last().map(|e| e.seq);
                (page, last)
            }
            View::Own | View::Subagent(_) => {
                let page = client.session_read_places(session_id, view.subagent(), Place::new(from), limit)?.0;
                let last = page.last().map(|e| e.place.get());
                (page, last)
            }
        };
        let Some(last) = last else { break };
        events.extend(page);
        from = last + 1;
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
