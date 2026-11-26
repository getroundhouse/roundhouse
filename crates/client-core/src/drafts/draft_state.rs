use super::keeper;
use crate::{Client, FileToSend};
use parking_lot::Mutex;
use protocol::model::{
    AttachmentId, CommentDraft, DecisionDraft, Draft, DraftToKeep, NewWorkspaceDraft, NewWorkspaceDraftToKeep, Rev,
    Session, SessionId, Workspace, WorkspaceId,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The longest a change waits to be kept while typing goes on, so typing a word isn't a call for each letter, and an
/// app that stops without quitting loses at most this much of it.
pub(crate) const PAUSE: Duration = Duration::from_millis(500);

/// Which draft: what's typed in a chat's composer, what's decided so far on the request its agent waits on, the comment
/// being written on a workspace's diff, or what's in the new-workspace dialog, which a person has one of. Each is kept
/// apart from the rest.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DraftKey {
    Composer(SessionId),
    Decision(SessionId),
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

/// What a draft says now, for the hub to keep in place of what it said: `None` for a decision or a comment with nothing
/// in it.
#[derive(Clone, Debug, PartialEq)]
pub enum DraftChange {
    Composer(SessionId, DraftToKeep),
    Decision(SessionId, Option<DecisionDraft>),
    Comment(WorkspaceId, Option<CommentDraft>),
    NewWorkspace(NewWorkspaceDraftToKeep),
}

impl DraftChange {
    pub(crate) fn key(&self) -> DraftKey {
        match self {
            Self::Composer(id, _) => DraftKey::Composer(id.clone()),
            Self::Decision(id, _) => DraftKey::Decision(id.clone()),
            Self::Comment(id, _) => DraftKey::Comment(id.clone()),
            Self::NewWorkspace(_) => DraftKey::NewWorkspace,
        }
    }
}

/// What a record says a draft is, and the record's revision, for a client to compare with the copy it agreed on, as
/// [`Drafts::from_elsewhere`] does, or to say it shows, as [`Drafts::took`] does.
#[derive(Clone)]
pub struct KeptDraft {
    pub(super) key: DraftKey,
    pub(super) copy: Agreed,
}

impl KeptDraft {
    /// The draft in the chat's composer, as its record says.
    pub fn composer(session: &Session) -> Self {
        let copy = Agreed { rev: session.rev, said: Said::Composer(session.draft.clone()) };
        Self { key: DraftKey::Composer(session.id.clone()), copy }
    }

    /// What's decided so far on the request the chat's agent waits on, as its record says.
    pub fn decision(session: &Session) -> Self {
        let copy = Agreed { rev: session.rev, said: Said::Decision(session.decision_draft.clone()) };
        Self { key: DraftKey::Decision(session.id.clone()), copy }
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
    pub(super) requests: flume::Sender<Request>,
    pub(super) counts: Counts,
    pub(super) caught_up: flume::Receiver<()>,
}

/// What the client and the thread share of each draft, under one lock, so a draft is never counted
/// as having nothing on its way before the copy its last change was kept as is known.
#[derive(Default)]
pub(super) struct Counted {
    /// How many of each draft's changes and files haven't reached the hub, counted as each is handed over and as the
    /// hub takes it, or it's replaced by a newer change. A draft with none has nothing that only this client has.
    pub(super) unkept: HashMap<DraftKey, usize>,
    /// The text of the newest change to each composer's draft that this client handed over, while any of the draft's
    /// changes and files haven't reached the hub, so a client shows what it handed over rather than the older record.
    /// It goes once the count does.
    pub(super) on_its_way: HashMap<DraftKey, String>,
    /// The newest copy of each draft that this client and the hub agree on: the one the hub gave back as it
    /// kept one of this client's changes there, or one this client took from the record.
    pub(super) agreed: HashMap<DraftKey, Agreed>,
    /// The composers whose last change from here the hub refused, since it has one typed after it, as `SessionDraft`
    /// says. So this client shows what the record says in place of its own, whatever that is, even what it agreed on
    /// already, until it takes a record or the hub keeps a later change from here.
    pub(super) overruled: HashSet<DraftKey>,
}

/// A copy of a draft, and the revision of the record it was on.
#[derive(Clone)]
pub(super) struct Agreed {
    pub(super) rev: Rev,
    pub(super) said: Said,
}

/// What a record says a draft is, as a client compares it with a copy it agreed on.
#[derive(Clone, PartialEq)]
pub(super) enum Said {
    Composer(Draft),
    Decision(Option<DecisionDraft>),
    Comment(Option<CommentDraft>),
    /// With no revision, which is the record's rather than what it says.
    NewWorkspace(NewWorkspaceDraft),
}

impl Said {
    pub(super) fn new_workspace(draft: &NewWorkspaceDraft) -> Self {
        Self::NewWorkspace(NewWorkspaceDraft { rev: Rev::default(), ..draft.clone() })
    }

    /// What a record says of a draft with nothing in it.
    pub(super) fn nothing(key: &DraftKey) -> Self {
        match key {
            DraftKey::Composer(_) => Self::Composer(Draft::default()),
            DraftKey::Decision(_) => Self::Decision(None),
            DraftKey::Comment(_) => Self::Comment(None),
            DraftKey::NewWorkspace => Self::NewWorkspace(NewWorkspaceDraft::default()),
        }
    }
}

pub(super) type Counts = Arc<Mutex<Counted>>;

pub(super) enum Request {
    /// With when it was handed over, which is when it was typed, by this device's clock.
    Changed {
        change: DraftChange,
        now: bool,
        typed_at_ms: i64,
    },
    Attach {
        to: FilesFor,
        file: FileToSend,
    },
    Detach {
        from: FilesFor,
        file: AttachmentId,
    },
    Reconnect(Client),
    Flush(flume::Sender<()>),
}

impl Drafts {
    /// Starts keeping drafts on `client`'s hub, for a client of its own, such as a test's playing another client. A
    /// view keeps its drafts with its [`Hub`](crate::Hub)'s.
    pub fn start(client: &Client) -> Self {
        Self::started(Some(client))
    }

    /// Starts keeping drafts on `connection`, or holding them until one is given to [`Drafts::reconnect`].
    pub(crate) fn started(connection: Option<&Client>) -> Self {
        let (requests, received) = flume::unbounded();
        let (catching_up, caught_up) = flume::bounded(1);
        let counts = Counts::default();
        keeper::start(connection.cloned(), received, counts.clone(), catching_up);
        Self { requests, counts, caught_up }
    }

    /// Carries on on a new connection, keeping first what the last one couldn't. It may be to another hub, whose
    /// revisions can be lower, and the dialog is the same draft on every hub, so its copy agreed on goes.
    pub(crate) fn reconnect(&self, client: &Client) {
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
    /// late. While one of this client's is on its way, nothing is, and [`Drafts::caught_up`] says once that one has
    /// reached the hub. The hub keeps a composer's change typed after the draft it has, so one typed before it, such as
    /// on a phone that couldn't reach the hub, is refused, and then the record's draft is from elsewhere whatever it
    /// says, empty or not, until this client takes one.
    pub fn from_elsewhere(&self, kept: &KeptDraft) -> bool {
        let counts = self.counts.lock();
        if counts.unkept.contains_key(&kept.key) {
            return false;
        }
        if counts.overruled.contains(&kept.key) {
            return true;
        }
        match counts.agreed.get(&kept.key) {
            Some(agreed) => kept.copy.rev > agreed.rev && kept.copy.said != agreed.said,
            None => kept.copy.said != Said::nothing(&kept.key),
        }
    }

    /// Says that this client shows the draft a record says, such as one it started with, or one made elsewhere that it
    /// took, so a newer record saying the same isn't from elsewhere.
    pub fn took(&self, kept: KeptDraft) {
        let mut counts = self.counts.lock();
        counts.overruled.remove(&kept.key);
        agree(&mut counts, kept.key, kept.copy);
    }

    /// Takes what `session`'s composer says to show it: the text this client last handed over for it, while that
    /// hasn't reached the hub, since `session`'s record is from before it, and otherwise what the record says, which
    /// this client then shows, as [`Drafts::took`] says. So a chat shown again before the hub has what was typed in it,
    /// such as within the pause before it's kept, or while the connection's gone, shows that rather than what was there
    /// before.
    pub fn take_composer(&self, session: &Session) -> String {
        let kept = KeptDraft::composer(session);
        let mut counts = self.counts.lock();
        if let Some(text) = counts.on_its_way.get(&kept.key) {
            return text.clone();
        }
        counts.overruled.remove(&kept.key);
        agree(&mut counts, kept.key, kept.copy);
        session.draft.text.clone()
    }

    /// Says that a draft now has nothing on its way to the hub, so a change made elsewhere that
    /// [`Drafts::from_elsewhere`] held back meanwhile can be shown. It holds one signal at a time, so signals nobody
    /// has looked at yet gather into one, and each goes to one receiver, so one reader listens.
    pub fn caught_up(&self) -> &flume::Receiver<()> {
        &self.caught_up
    }

    pub(super) fn change(&self, change: DraftChange, now: bool) {
        let key = change.key();
        {
            let mut counts = self.counts.lock();
            count_handed(&mut counts, &key);
            if let DraftChange::Composer(_, draft) = &change {
                counts.on_its_way.insert(key, draft.text.clone());
            }
        }
        let _ = self.requests.send(Request::Changed { change, now, typed_at_ms: typed_now() });
    }

    pub(super) fn handed(&self, key: &DraftKey) {
        count_handed(&mut self.counts.lock(), key);
    }
}

/// Now, by this device's clock, in milliseconds since the Unix epoch. A change is stamped as it's handed over, rather
/// than as it's sent, so one that waits for a connection, such as on a phone away from the hub, keeps when it was
/// typed.
fn typed_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// Counts one more of `key`'s changes or files on its way, before it's sent, so the thread can't take it, and count it
/// down, first.
fn count_handed(counts: &mut Counted, key: &DraftKey) {
    *counts.unkept.entry(key.clone()).or_default() += 1;
}

/// Takes `copy` as the one the client and the hub agree on for `key`, unless the one it has is newer.
pub(super) fn agree(counts: &mut Counted, key: DraftKey, copy: Agreed) {
    if counts.agreed.get(&key).is_none_or(|agreed| copy.rev > agreed.rev) {
        counts.agreed.insert(key, copy);
    }
}
