//! The feed: the hub's notices, for every client following it, leaving out any copy of a record that's no newer than
//! one it has told of.

use std::collections::HashMap;

use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{RecordKey, Rev, SessionId, ToolUseId};
use tokio::sync::broadcast;
use tracing::trace;

#[cfg(test)]
mod tests;

/// Status changes are rare, so a client only overflows this if it has stopped reading.
const CAPACITY: usize = 256;

/// Which record a notice is of. A subagent's is its call in its session, since it has no key of its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Of {
    Record(RecordKey),
    Subagent(SessionId, ToolUseId),
}

pub(super) struct Feed {
    sender: broadcast::Sender<HubNotice>,
    /// The revision of the newest copy sent of each record, or of its removal. It keeps one for each record the feed
    /// has told of since the hub started, since a late copy of one removed long ago is still older.
    sent: Mutex<HashMap<Of, Rev>>,
}

impl Feed {
    pub(super) fn new() -> Self {
        Self { sender: broadcast::channel(CAPACITY).0, sent: Mutex::default() }
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<HubNotice> {
        self.sender.subscribe()
    }

    /// Tells every client following the feed of `notice`, unless it's a copy of a record no newer than one it has told
    /// of, or than that record's removal, so each copy of a record that a client hears is newer than the last. Several
    /// tasks can announce one record, each with a copy read at or after its own change, so two can carry the same copy,
    /// or finish in the other order, and a change made while another's copy was read is told of only by the newer
    /// copy. A client would drop an older copy anyway, as [`Rev::supersedes`] says. The check and the sending happen
    /// under one lock, so the order they're checked in is the order clients hear them.
    pub(super) fn send(&self, notice: HubNotice) {
        let mut sent = self.sent.lock();
        if let Some((of, rev)) = of(&notice) {
            let newest = sent.entry(of).or_default();
            // Only a hub that doesn't keep revisions makes a copy without one, which is always sent, as a client always
            // takes it.
            if !rev.supersedes(*newest) {
                trace!(rev = rev.get(), newest = newest.get(), "left out a copy no newer than one told of");
                return;
            }
            *newest = rev.max(*newest);
        }
        // An error only means nobody is listening.
        let _ = self.sender.send(notice);
    }
}

/// Which record `notice` is of, and the revision of what it says, or `None` for a notice that isn't of one.
fn of(notice: &HubNotice) -> Option<(Of, Rev)> {
    let record = |key, rev| Some((Of::Record(key), rev));
    match notice {
        HubNotice::Repo(repo) => record(RecordKey::Repo { id: repo.id.clone() }, repo.rev),
        HubNotice::Workspace(workspace) => record(RecordKey::Workspace { id: workspace.id.clone() }, workspace.rev),
        HubNotice::Session(session) => record(RecordKey::Session { id: session.id.clone() }, session.rev),
        HubNotice::Comment(comment) => record(RecordKey::Comment { id: comment.id.clone() }, comment.rev),
        HubNotice::HubSettings(settings) => record(RecordKey::HubSettings, settings.rev),
        HubNotice::Account(account) => record(RecordKey::Account { id: account.id.clone() }, account.rev),
        HubNotice::Subagent(subagent) => {
            Some((Of::Subagent(subagent.session_id.clone(), subagent.call.clone()), subagent.rev))
        }
        HubNotice::Removed { record: key, rev } => record(key.clone(), *rev),
        HubNotice::Resync | HubNotice::Unknown => None,
    }
}
