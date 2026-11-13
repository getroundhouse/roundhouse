use crate::patch;
use crate::patch::FilePatch;
use client_core::{Client, RecordsFeed};
use gpui_kit::{AppContext as _, Context, EventEmitter};
use protocol::model::{CommentDraft, CommentId, CommitId, DiffComment, WorkspaceId};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum ReviewEvent {
    Error(String),
    /// The diff has loaded again.
    Loaded,
    /// Send this to the agent, in the chat showing.
    SendToAgent(String),
    /// The comment being written was saved, which took it off the hub.
    Saved,
    /// The comment being written couldn't be saved, so here it is back, which the hub still keeps.
    NotSaved(CommentDraft),
}

pub struct Review {
    pub(super) client: Client,
    /// Client-core's copy of the hub's records, which replies go into.
    pub(super) records: RecordsFeed,
    pub(super) workspace_id: WorkspaceId,
    /// Every changed file, in order of path, or `None` until the diff has loaded.
    pub files: Option<Vec<FilePatch>>,
    /// The commit the diff was taken from, which holds each file as it was before the change. `None` until the diff
    /// has loaded, or from a worker too old to say.
    pub base_commit: Option<CommitId>,
    /// The hub left some untracked files out.
    pub truncated: bool,
    /// How many times the diff has been asked for, so only the newest answer is taken, and an older one that comes
    /// late isn't shown over it, or taken for what the workspace changes now.
    pub(super) asks: u64,
    /// What's shown: client-core's copy, less what's being deleted here.
    pub comments: Vec<DiffComment>,
    /// Client-core's copy.
    pub(super) held: Vec<DiffComment>,
    /// Deleted here, and still in client-core's copy until the feed says they've gone.
    pub(super) deleting: HashSet<CommentId>,
}

impl EventEmitter<ReviewEvent> for Review {}

impl Review {
    /// Starts loading the diff, with the comments client-core holds for the workspace now.
    pub fn new(
        client: Client,
        records: RecordsFeed,
        workspace_id: WorkspaceId,
        held: Vec<DiffComment>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            client,
            records,
            workspace_id,
            files: None,
            base_commit: None,
            truncated: false,
            asks: 0,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
        };
        this.set_comments(held, cx);
        this.refresh(cx);
        this
    }

    /// Carries on on a new connection to the hub. The diff is loaded again on it with the next [`Self::refresh`].
    pub fn reconnect(&mut self, client: &Client) {
        self.client = client.clone();
    }

    /// Loads the diff again. Keeps the old one until the new one is ready.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asks += 1;
        let (ask, client, id) = (self.asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx.background_spawn(async move { client.workspace_diff(&id) }).await;
            let parsed = match loaded {
                Ok(diff) => {
                    let (truncated, base) = (diff.truncated, diff.base_commit);
                    let parsing = cx.background_spawn(async move {
                        let mut files =
                            patch::parse(&diff.patch).map_err(|e| format!("couldn't read the diff: {e}"))?;
                        files.sort_by(|a, b| a.path().cmp(b.path()));
                        Ok::<_, String>((files, truncated, base))
                    });
                    parsing.await
                }
                Err(e) => Err(format!("couldn't load the diff: {e}")),
            };
            let _ = this.update(cx, |t, cx| {
                if ask != t.asks {
                    return;
                }
                match parsed {
                    Ok((files, truncated, base)) => {
                        t.files = Some(files);
                        t.truncated = truncated;
                        t.base_commit = base;
                        cx.emit(ReviewEvent::Loaded);
                    }
                    Err(message) => cx.emit(ReviewEvent::Error(message)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The connection the diff is read on, and its workspace, for reading what each file it changes holds.
    pub(crate) fn source(&self) -> (Client, WorkspaceId) {
        (self.client.clone(), self.workspace_id.clone())
    }

    /// Whether the diff has loaded, with nothing changed.
    pub fn has_no_changes(&self) -> bool {
        self.files.as_ref().is_some_and(Vec::is_empty)
    }

    /// A file as the diff last loaded has it. `None` once it no longer changes anything.
    pub fn file(&self, path: &str) -> Option<&FilePatch> {
        self.files.as_ref()?.iter().find(|file| file.path().as_ref() == path)
    }

    /// A comment that's shown.
    pub fn comment(&self, id: &CommentId) -> Option<&DiffComment> {
        self.comments.iter().find(|c| &c.id == id)
    }
}
