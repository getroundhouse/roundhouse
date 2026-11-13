//! Keeping what's typed and not sent on the hub, so it's there after the app restarts, and in every client: what's
//! typed and attached in each chat's composer, what's chosen and typed in answer to each chat's question, the comment
//! being written on each workspace's diff, and what's in the new-workspace dialog, files included. A client hands over
//! each change as it's made, and a thread of its own keeps the newest of each, one call at a time, so they reach the
//! hub in the order they were made. Each file goes before any draft that names it, since the hub leaves out a file it
//! hasn't got, and one let go of goes after the draft that stopped naming it, since the hub keeps any a draft names.
//! What a hub can't take, because the connection went or it doesn't keep that kind, waits for the next connection. The
//! count of what hasn't reached the hub, for each draft, says what quitting would lose, and quitting
//! waits for it with [`Drafts::flushed`], which needs no other thread. The same count, and the last copy of what's
//! there that this client and the hub agree on, say whether what's on the hub was made somewhere else, such as in
//! another client, as [`Drafts::from_elsewhere`] says.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use protocol::model::{
    AnswerDraft, AttachmentId, CommentDraft, Draft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep, Rev,
    Session, SessionId, Workspace, WorkspaceId,
};

use crate::{Client, FileToSend};

mod keeper;
mod read_back;
#[cfg(test)]
mod tests;

pub use read_back::{CameBack, NotReadBack, ReadBack};

/// The longest a change waits to be kept while typing goes on, so typing a word isn't a call for each letter, and an
/// app that stops without quitting loses at most this much of it.
pub(crate) const PAUSE: Duration = Duration::from_millis(500);

/// Which draft: what's typed in a chat's composer, its answer to its question, the comment being written on a
/// workspace's diff, or what's in the new-workspace dialog, which a person has one of. Each is kept apart from the rest.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DraftKey {
    Composer(SessionId),
    Answer(SessionId),
    Comment(WorkspaceId),
    NewWorkspace,
}

/// Which draft files are attached for, to name: a chat's composer, or the new-workspace dialog. A chat's are added to
/// the chat, and the dialog's are kept apart, since no chat exists yet to add them to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FilesFor {
    Chat(SessionId),
    NewWorkspace,
}

impl FilesFor {
    /// The draft that names them.
    pub(crate) fn key(&self) -> DraftKey {
        match self {
            Self::Chat(id) => DraftKey::Composer(id.clone()),
            Self::NewWorkspace => DraftKey::NewWorkspace,
        }
    }
}

/// What a draft says now, for the hub to keep in place of what it said: `None` for an answer or a comment with nothing
/// in it.
#[derive(Clone, Debug, PartialEq)]
pub enum DraftChange {
    Composer(SessionId, DraftToKeep),
    Answer(SessionId, Option<AnswerDraft>),
    Comment(WorkspaceId, Option<CommentDraft>),
    NewWorkspace(NewWorkspaceDraftToKeep),
}

impl DraftChange {
    pub(crate) fn key(&self) -> DraftKey {
        match self {
            Self::Composer(id, _) => DraftKey::Composer(id.clone()),
            Self::Answer(id, _) => DraftKey::Answer(id.clone()),
            Self::Comment(id, _) => DraftKey::Comment(id.clone()),
            Self::NewWorkspace(_) => DraftKey::NewWorkspace,
        }
    }
}

/// What a record says a draft is, and the record's revision, for a client to compare with the copy it agreed on, as
/// [`Drafts::from_elsewhere`] does, or to say it shows, as [`Drafts::took`] does.
#[derive(Clone)]
pub struct KeptDraft {
    key: DraftKey,
    copy: Agreed,
}

impl KeptDraft {
    /// The draft in the chat's composer, as its record says.
    pub fn composer(session: &Session) -> Self {
        let copy = Agreed { rev: session.rev, said: Said::Composer(session.draft.clone()) };
        Self { key: DraftKey::Composer(session.id.clone()), copy }
    }

    /// What's chosen and typed in answer to the chat's question, as its record says.
    pub fn answer(session: &Session) -> Self {
        let copy = Agreed { rev: session.rev, said: Said::Answer(session.answer_draft.clone()) };
        Self { key: DraftKey::Answer(session.id.clone()), copy }
    }

    /// The comment being written on the workspace's diff, as its record says.
    pub fn comment(workspace: &Workspace) -> Self {
        let copy = Agreed { rev: workspace.rev, said: Said::Comment(workspace.comment_draft.clone()) };
        Self { key: DraftKey::Comment(workspace.id.clone()), copy }
    }

    /// What's in the new-workspace dialog, which is a record of its own.
    pub fn new_workspace(draft: &NewWorkspaceDraft) -> Self {
        Self { key: DraftKey::NewWorkspace, copy: Agreed { rev: draft.rev, said: Said::new_workspace(draft) } }
    }
}

/// Keeps drafts on the hub. A clone is another handle on the same thread, which ends once every handle has gone.
#[derive(Clone)]
pub struct Drafts {
    requests: flume::Sender<Request>,
    counts: Counts,
    caught_up: flume::Receiver<()>,
}

/// What the client and the thread share of each draft, under one lock, so a draft is never counted
/// as having nothing on its way before the copy its last change was kept as is known.
#[derive(Default)]
struct Counted {
    /// How many of each draft's changes and files haven't reached the hub, counted as each is handed over and as the
    /// hub takes it, or it's replaced by a newer change. A draft with none has nothing that only this client has.
    unkept: HashMap<DraftKey, usize>,
    /// The newest copy of each draft that this client and the hub agree on: the one the hub gave back as it
    /// kept one of this client's changes there, or one this client took from the record.
    agreed: HashMap<DraftKey, Agreed>,
}

/// A copy of a draft, and the revision of the record it was on.
#[derive(Clone)]
struct Agreed {
    rev: Rev,
    said: Said,
}

/// What a record says a draft is, as a client compares it with a copy it agreed on.
#[derive(Clone, PartialEq)]
enum Said {
    Composer(Draft),
    Answer(Option<AnswerDraft>),
    Comment(Option<CommentDraft>),
    /// With no revision, which is the record's rather than what it says.
    NewWorkspace(NewWorkspaceDraft),
}

impl Said {
    fn new_workspace(draft: &NewWorkspaceDraft) -> Self {
        Self::NewWorkspace(NewWorkspaceDraft { rev: Rev::default(), ..draft.clone() })
    }

    /// What a record says of a draft with nothing in it.
    fn nothing(key: &DraftKey) -> Self {
        match key {
            DraftKey::Composer(_) => Self::Composer(Draft::default()),
            DraftKey::Answer(_) => Self::Answer(None),
            DraftKey::Comment(_) => Self::Comment(None),
            DraftKey::NewWorkspace => Self::NewWorkspace(NewWorkspaceDraft::default()),
        }
    }
}

type Counts = Arc<Mutex<Counted>>;

enum Request {
    Changed { change: DraftChange, now: bool },
    Attach { to: FilesFor, file: FileToSend },
    Detach { from: FilesFor, file: AttachmentId },
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

    /// Carries on on a new connection, keeping first what the last one couldn't. It may be to another hub, whose
    /// revisions can be lower, and the dialog is the same draft on every hub, so its copy agreed on goes.
    pub fn reconnect(&self, client: &Client) {
        self.counts.lock().agreed.remove(&DraftKey::NewWorkspace);
        let _ = self.requests.send(Request::Reconnect(client.clone()));
    }

    /// What a draft says now, kept within [`PAUSE`] while typing goes on.
    pub fn typed(&self, change: DraftChange) {
        self.change(change, false);
    }

    /// The same, kept at once, for a change nobody's still typing, such as the composer emptied by sending, or
    /// something picked rather than typed.
    pub fn set(&self, change: DraftChange) {
        self.change(change, true);
    }

    /// A file for a draft to name, given to the hub before any draft given after this is kept. A chat's is added to
    /// the chat, and the new-workspace dialog's is kept until it's let go of.
    pub fn attach(&self, to: FilesFor, file: FileToSend) {
        self.handed(&to.key());
        let _ = self.requests.send(Request::Attach { to, file });
    }

    /// Lets go of a file the draft no longer names, once the draft given before this is kept.
    pub fn detach(&self, from: FilesFor, file: &AttachmentId) {
        let _ = self.requests.send(Request::Detach { from, file: file.clone() });
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

    /// Whether something handed over for the draft, a change or a file, hasn't reached the hub yet, and would be lost
    /// if the app stopped now.
    pub fn unkept(&self, key: &DraftKey) -> bool {
        self.counts.lock().unkept.contains_key(key)
    }

    /// Whether the draft a record says was made somewhere else, such as in another client, so a client shows it in
    /// place of its own. Nothing handed over for the draft is still on its way, and the record is newer than the last
    /// copy of the draft that this client kept or took from it, as [`Drafts::took`] says, and says something else. So a
    /// record that's newer for something else, such as a chat's status, isn't, and nor is an older one that arrives
    /// late. While one of this client's is on its way, that one goes on and replaces whatever was made elsewhere
    /// meanwhile, and [`Drafts::caught_up`] says once it has arrived.
    pub fn from_elsewhere(&self, kept: &KeptDraft) -> bool {
        let counts = self.counts.lock();
        if counts.unkept.contains_key(&kept.key) {
            return false;
        }
        match counts.agreed.get(&kept.key) {
            Some(agreed) => kept.copy.rev > agreed.rev && kept.copy.said != agreed.said,
            None => kept.copy.said != Said::nothing(&kept.key),
        }
    }

    /// Says that this client shows the draft a record says, such as one it started with, or one made elsewhere that it
    /// took, so a newer record saying the same isn't from elsewhere.
    pub fn took(&self, kept: KeptDraft) {
        agree(&mut self.counts.lock(), kept.key, kept.copy);
    }

    /// Says that a draft now has nothing on its way to the hub, so a change made elsewhere that
    /// [`Drafts::from_elsewhere`] held back meanwhile can be shown. It holds one signal at a time, so signals nobody has
    /// looked at yet gather into one, and each goes to one receiver, so one reader listens.
    pub fn caught_up(&self) -> &flume::Receiver<()> {
        &self.caught_up
    }

    fn change(&self, change: DraftChange, now: bool) {
        self.handed(&change.key());
        let _ = self.requests.send(Request::Changed { change, now });
    }

    /// Counted before it's sent, so the thread can't take it, and count it down, first.
    fn handed(&self, key: &DraftKey) {
        *self.counts.lock().unkept.entry(key.clone()).or_default() += 1;
    }
}

/// Takes `copy` as the one the client and the hub agree on for `key`, unless the one it has is newer.
fn agree(counts: &mut Counted, key: DraftKey, copy: Agreed) {
    if counts.agreed.get(&key).is_none_or(|agreed| copy.rev > agreed.rev) {
        counts.agreed.insert(key, copy);
    }
}
