//! Keeping what's typed and not sent on the hub, so it's there after the app restarts, and in every client: what's
//! typed and attached in each chat's composer, what's chosen and typed in answer to each chat's question, and the
//! comment being written on each workspace's diff. A client hands over each change as it's made, and a thread of its
//! own keeps the newest of each, one call at a time, so they reach the hub in the order they were made. Each file goes
//! before any draft that names it, since the hub leaves out a file it hasn't got, and one let go of goes after the
//! draft that stopped naming it, since the hub keeps any a draft names. What a hub can't take, because the connection
//! went or it doesn't keep that kind, waits for the next connection. The count of what hasn't reached the hub, for each
//! place something's typed, says what quitting would lose, and quitting waits for it with [`Drafts::flushed`], which
//! needs no other thread. The same count, and the last copy of what's there that this client and the hub agree on, say
//! whether what's on the hub was made somewhere else, such as in another client, as [`Drafts::from_elsewhere`] says.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use protocol::model::{
    AnswerDraft, AttachmentId, CommentDraft, Draft, NewDraft, Rev, Session, SessionId, Workspace, WorkspaceId,
};

use crate::{Client, FileToSend};

mod keeper;
#[cfg(test)]
mod tests;

/// The longest a change waits to be kept while typing goes on, so typing a word isn't a call for each letter, and an
/// app that stops without quitting loses at most this much of it.
pub(crate) const PAUSE: Duration = Duration::from_millis(500);

/// Where something's typed, each kept apart from the rest: a chat's composer, its answer to its question, or the
/// comment being written on a workspace's diff.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Where {
    Composer(SessionId),
    Answer(SessionId),
    Comment(WorkspaceId),
}

/// What's typed in one place now, for the hub to keep in place of what was.
#[derive(Clone, Debug, PartialEq)]
enum Change {
    Composer(SessionId, NewDraft),
    Answer(SessionId, Option<AnswerDraft>),
    Comment(WorkspaceId, Option<CommentDraft>),
}

impl Change {
    fn at(&self) -> Where {
        match self {
            Self::Composer(id, _) => Where::Composer(id.clone()),
            Self::Answer(id, _) => Where::Answer(id.clone()),
            Self::Comment(id, _) => Where::Comment(id.clone()),
        }
    }
}

/// Keeps drafts on the hub. A clone is another handle on the same thread, which ends once every handle has gone.
#[derive(Clone)]
pub struct Drafts {
    requests: flume::Sender<Request>,
    counts: Counts,
    caught_up: flume::Receiver<()>,
}

/// What the client and the thread share of each place something's typed, under one lock, so a place is never counted
/// as having nothing on its way before the copy its last change was kept as is known.
#[derive(Default)]
struct Counted {
    /// How many of each place's changes and files haven't reached the hub, counted as each is handed over and as the
    /// hub takes it, or it's replaced by a newer change. A place with none has nothing that only this client has.
    unkept: HashMap<Where, usize>,
    /// The newest copy of what's in each place that this client and the hub agree on: the one the hub gave back as it
    /// kept one of this client's changes there, or one this client took from the record.
    agreed: HashMap<Where, Agreed>,
}

/// A copy of what's in one place, and the revision of the record it was on.
#[derive(Clone)]
struct Agreed {
    rev: Rev,
    said: Said,
}

/// What a record says is in one place, as a client compares it with a copy it agreed on.
#[derive(Clone, PartialEq)]
enum Said {
    Composer(Draft),
    Answer(Option<AnswerDraft>),
    Comment(Option<CommentDraft>),
}

impl Said {
    /// What a record says of a place where nothing's typed.
    fn nothing(at: &Where) -> Self {
        match at {
            Where::Composer(_) => Self::Composer(Draft::default()),
            Where::Answer(_) => Self::Answer(None),
            Where::Comment(_) => Self::Comment(None),
        }
    }
}

type Counts = Arc<Mutex<Counted>>;

enum Request {
    Changed { change: Change, now: bool },
    Attach { id: SessionId, file: FileToSend },
    Detach { id: SessionId, file: AttachmentId },
    Reconnect(Client),
    Flush(flume::Sender<()>),
}

impl Drafts {
    /// Starts keeping drafts on `client`'s hub.
    pub fn start(client: &Client) -> Self {
        let (requests, received) = flume::unbounded();
        let (catching_up, caught_up) = flume::bounded(1);
        let counts = Counts::default();
        keeper::start(client.clone(), received, counts.clone(), catching_up);
        Self { requests, counts, caught_up }
    }

    /// Carries on on a new connection, keeping first what the last one couldn't.
    pub fn reconnect(&self, client: &Client) {
        let _ = self.requests.send(Request::Reconnect(client.clone()));
    }

    /// What's typed in the chat's composer now, kept within [`PAUSE`].
    pub fn typed(&self, id: &SessionId, draft: NewDraft) {
        self.change(Change::Composer(id.clone(), draft), false);
    }

    /// The same, kept at once, for a change nobody's still typing, such as the composer emptied by sending.
    pub fn set(&self, id: &SessionId, draft: NewDraft) {
        self.change(Change::Composer(id.clone(), draft), true);
    }

    /// What's chosen and typed in answer to the chat's question now, kept within [`PAUSE`], or `None` for nothing.
    pub fn typed_answer(&self, id: &SessionId, draft: Option<AnswerDraft>) {
        self.change(Change::Answer(id.clone(), draft), false);
    }

    /// The same, kept at once, such as `None` as the answer is sent, or something chosen rather than typed.
    pub fn set_answer(&self, id: &SessionId, draft: Option<AnswerDraft>) {
        self.change(Change::Answer(id.clone(), draft), true);
    }

    /// The comment being written on the workspace's diff now, kept within [`PAUSE`], or `None` for none.
    pub fn typed_comment(&self, id: &WorkspaceId, draft: Option<CommentDraft>) {
        self.change(Change::Comment(id.clone(), draft), false);
    }

    /// The same, kept at once, such as `None` as the comment is saved or cancelled.
    pub fn set_comment(&self, id: &WorkspaceId, draft: Option<CommentDraft>) {
        self.change(Change::Comment(id.clone(), draft), true);
    }

    /// A file for the chat's drafts to name, added to the chat before any draft given after this is kept.
    pub fn attach(&self, id: &SessionId, file: FileToSend) {
        self.handed(&Where::Composer(id.clone()));
        let _ = self.requests.send(Request::Attach { id: id.clone(), file });
    }

    /// Lets go of a file the chat's draft no longer names, once the draft given before this is kept.
    pub fn detach(&self, id: &SessionId, file: &AttachmentId) {
        let _ = self.requests.send(Request::Detach { id: id.clone(), file: file.clone() });
    }

    /// Keeps everything given so far at once, and hears once it's kept, or once that's as far as it can get, such as
    /// without a connection.
    pub fn flushed(&self) -> flume::Receiver<()> {
        let (done, flushed) = flume::bounded(1);
        if let Err(flume::SendError(Request::Flush(done))) = self.requests.send(Request::Flush(done)) {
            let _ = done.send(());
        }
        flushed
    }

    /// Whether something handed over for the chat's composer, a draft or a file, hasn't reached the hub yet, and would
    /// be lost if the app stopped now.
    pub fn unkept(&self, id: &SessionId) -> bool {
        self.unkept_in(&Where::Composer(id.clone()))
    }

    /// The same, for any place something's typed.
    pub fn unkept_in(&self, at: &Where) -> bool {
        self.counts.lock().unkept.contains_key(at)
    }

    /// Whether the draft on the hub's `session` record was made somewhere else, such as in another client, so a client
    /// shows it in place of its own. Nothing handed over for the chat's composer is still on its way, and the record is
    /// newer than the last copy of its draft that this client kept there or took from it, as [`Drafts::took`] says, and
    /// its draft isn't that copy. So a record that's newer for something else, such as its status, isn't, and nor is an
    /// older one that arrives late. While one of this client's is on its way, that one goes on and replaces whatever
    /// was made elsewhere meanwhile, and [`Drafts::caught_up`] says once it has arrived.
    pub fn from_elsewhere(&self, session: &Session) -> bool {
        self.elsewhere(&Where::Composer(session.id.clone()), session.rev, &Said::Composer(session.draft.clone()))
    }

    /// The same, for what's chosen and typed in answer to the chat's question.
    pub fn answer_from_elsewhere(&self, session: &Session) -> bool {
        self.elsewhere(&Where::Answer(session.id.clone()), session.rev, &Said::Answer(session.answer_draft.clone()))
    }

    /// The same, for the comment being written on the workspace's diff.
    pub fn comment_from_elsewhere(&self, workspace: &Workspace) -> bool {
        let said = Said::Comment(workspace.comment_draft.clone());
        self.elsewhere(&Where::Comment(workspace.id.clone()), workspace.rev, &said)
    }

    /// Says that this client shows the draft on `session`'s record, such as one it started with, or one made
    /// elsewhere that it took, so a newer record carrying the same draft isn't from elsewhere.
    pub fn took(&self, session: &Session) {
        let said = Said::Composer(session.draft.clone());
        self.agree(Where::Composer(session.id.clone()), Agreed { rev: session.rev, said });
    }

    /// The same, for what's chosen and typed in answer to the chat's question.
    pub fn took_answer(&self, session: &Session) {
        let said = Said::Answer(session.answer_draft.clone());
        self.agree(Where::Answer(session.id.clone()), Agreed { rev: session.rev, said });
    }

    /// The same, for the comment being written on the workspace's diff.
    pub fn took_comment(&self, workspace: &Workspace) {
        let said = Said::Comment(workspace.comment_draft.clone());
        self.agree(Where::Comment(workspace.id.clone()), Agreed { rev: workspace.rev, said });
    }

    /// Says that a place something's typed now has nothing on its way to the hub, so a change made elsewhere that
    /// [`Drafts::from_elsewhere`] held back meanwhile can be shown. It holds one signal at a time, so signals nobody has
    /// looked at yet gather into one, and each goes to one receiver, so one reader listens.
    pub fn caught_up(&self) -> &flume::Receiver<()> {
        &self.caught_up
    }

    /// Whether a record at `rev` that says `said` is at `at` is newer than the copy this client agreed on there, and
    /// says something else, with nothing from here still on its way.
    fn elsewhere(&self, at: &Where, rev: Rev, said: &Said) -> bool {
        let counts = self.counts.lock();
        if counts.unkept.contains_key(at) {
            return false;
        }
        match counts.agreed.get(at) {
            Some(agreed) => rev > agreed.rev && *said != agreed.said,
            None => *said != Said::nothing(at),
        }
    }

    fn agree(&self, at: Where, copy: Agreed) {
        agree(&mut self.counts.lock(), at, copy);
    }

    fn change(&self, change: Change, now: bool) {
        self.handed(&change.at());
        let _ = self.requests.send(Request::Changed { change, now });
    }

    /// Counted before it's sent, so the thread can't take it, and count it down, first.
    fn handed(&self, at: &Where) {
        *self.counts.lock().unkept.entry(at.clone()).or_default() += 1;
    }
}

/// Takes `copy` as the one the client and the hub agree on at `at`, unless the one it has is newer.
fn agree(counts: &mut Counted, at: Where, copy: Agreed) {
    if counts.agreed.get(&at).is_none_or(|agreed| copy.rev > agreed.rev) {
        counts.agreed.insert(at, copy);
    }
}
