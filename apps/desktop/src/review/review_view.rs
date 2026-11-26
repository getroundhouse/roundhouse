use super::listed::Summary;
use super::listing::{ChangedEntry, Listing};
use client_core::Hub;
use gpui_kit::{Context, EventEmitter};
use protocol::model::{CommentDraft, CommentId, DiffComment, WorkspaceId};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum ReviewEvent {
    Error(String),
    /// The file the file tab shows has been read.
    Loaded,
    /// What's listed says the workspace changes this many files, as the summary is taken in, and at each moment that
    /// reads the changes again.
    Counted(usize),
    /// The comment being written couldn't be saved, so here it is back, which the hub still keeps.
    NotSaved(CommentDraft),
}

pub struct Review {
    /// What the diff is read from, and comments are saved to.
    pub(super) hub: Hub,
    pub(super) workspace_id: WorkspaceId,
    /// What Changes lists, and the file the file tab shows, read with it. `None` until the workspace's level has listed
    /// its changes.
    pub(super) listed: Option<Summary>,
    /// What Changes lists, laid out from `listed` again only when that changes.
    pub(super) listing: Option<Listing>,
    /// The file the file tab shows the diff of, which is read on its own.
    pub(super) diffed: Option<ChangedEntry>,
    /// How many times one file's diff has been asked for, so only the newest answer is taken.
    pub(super) one_file_asks: u64,
    /// What's shown: client-core's copy, less what's being deleted here.
    pub comments: Vec<DiffComment>,
    /// Client-core's copy.
    pub(super) held: Vec<DiffComment>,
    /// Deleted here, and still in client-core's copy until the feed says they've gone.
    pub(super) deleting: HashSet<CommentId>,
    /// Whether the worktree can be read, as the pane says. While it can't, as once the workspace is being archived,
    /// which the hub refuses every read of, nothing is read, and what was read stays.
    pub(super) worktree_readable: bool,
}

impl EventEmitter<ReviewEvent> for Review {}

impl Review {
    /// With the comments client-core holds for the workspace now, and nothing read: [`Self::take_summary`] lists what
    /// client-core holds of the changed files, while the worktree can be read, as `worktree_readable` says to start
    /// with.
    pub fn new(
        hub: Hub,
        workspace_id: WorkspaceId,
        held: Vec<DiffComment>,
        worktree_readable: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            hub,
            workspace_id,
            listed: None,
            listing: None,
            diffed: None,
            one_file_asks: 0,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
            worktree_readable,
        };
        this.set_comments(held, cx);
        this
    }

    /// Says again how many files the workspace changes, and reads the file the file tab shows again if its last read
    /// failed, at each moment that would read the changes again, as the pane shows or a turn ends. Nothing else is
    /// read, since Changes lists what the workspace's level sends. It needs nothing more once a new connection has
    /// caught up, since what it lists came with the hub's records, so a reconnect doesn't start a worker for every
    /// pane at once.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.say_what_changes(cx);
        self.read_the_file_again_if_it_moved(cx);
    }

    /// Tells whoever listens how many files the workspace changes, as what's listed says, once it's listed.
    pub(super) fn say_what_changes(&self, cx: &mut Context<Self>) {
        if let Some(listing) = &self.listing {
            cx.emit(ReviewEvent::Counted(listing.changed()));
        }
    }

    /// Takes in whether the worktree can be read, which the pane knows from the workspace's status. Nothing is read
    /// while it can't, so a file clicked then says it couldn't be read, rather than the hub refusing a read of it.
    pub fn set_worktree_readable(&mut self, readable: bool, cx: &mut Context<Self>) {
        if self.worktree_readable != readable {
            self.worktree_readable = readable;
            cx.notify();
        }
    }

    /// What the diff is read from, and its workspace, for reading what each file it changes holds.
    pub(crate) fn source(&self) -> (Hub, WorkspaceId) {
        (self.hub.clone(), self.workspace_id.clone())
    }

    /// Whether the changes have been listed, and the workspace changes nothing.
    pub fn has_no_changes(&self) -> bool {
        self.listing.as_ref().is_some_and(|listing| listing.changed() == 0)
    }

    /// A comment that's shown.
    pub fn comment(&self, id: &CommentId) -> Option<&DiffComment> {
        self.comments.iter().find(|c| &c.id == id)
    }
}
