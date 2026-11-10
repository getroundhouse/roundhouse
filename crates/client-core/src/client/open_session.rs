//! One session's log, kept current so a view only reads it: a window of its history and its live tail.

use parking_lot::Mutex;
use protocol::attachment::Attachment;
use protocol::event::SessionEvent;
use protocol::model::{ClientMessageId, Seq, Session, SessionId};

use super::follow::{Follow, FollowError, Source};
use super::{Client, Result, Subscription, SubscriptionItem};
use crate::session_log::{Changes, PendingPrompt, SessionLog};

/// A session someone has open, kept current until every clone of this has gone. It fetches the history a
/// view is looking at, and carries on from a new connection given to [`OpenSession::reconnect`].
#[derive(Clone)]
pub struct OpenSession {
    follow: Follow<SessionLog>,
}

impl OpenSession {
    /// Starts following, on a thread of its own, so this doesn't wait for the hub. Once it has subscribed, it asks for
    /// the session's prompts.
    pub fn start(client: &Client, session: &Session) -> OpenSession {
        let source = SessionSource { session_id: session.id.clone() };
        let follow = Follow::start(client, SessionLog::new(session.last_seq), source);
        follow.wake();
        OpenSession { follow }
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
        self.follow.reconnect(client);
    }

    pub fn take_errors(&self) -> Vec<FollowError> {
        self.follow.take_errors()
    }
}

struct SessionSource {
    session_id: SessionId,
}

impl Source for SessionSource {
    type Held = SessionLog;

    fn resume(&mut self, client: &Client, held: &Mutex<SessionLog>) -> Result<Subscription> {
        // Events arrive in order with no gaps, so carrying on from one past the last misses nothing, whatever
        // was fetched meanwhile.
        let from = held.lock().received() + 1;
        let subscription = client.session_subscribe(&self.session_id, from)?;
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
            match read_range(client, &self.session_id, range) {
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
    /// on either way.
    fn list_prompts(&self, client: &Client, held: &Mutex<SessionLog>) -> Result<bool> {
        if !held.lock().wants_prompt_list() {
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

/// A size limit can cut a read short, so this keeps going until the range is complete.
fn read_range(client: &Client, session_id: &SessionId, range: std::ops::Range<Seq>) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();
    let mut from = range.start;
    while from < range.end {
        let limit = u32::try_from(range.end - from).unwrap_or(u32::MAX);
        let (page, _) = client.session_read(session_id, from, limit, false)?;
        let Some(last) = page.last().map(|e| e.seq) else { break };
        events.extend(page);
        from = last + 1;
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
