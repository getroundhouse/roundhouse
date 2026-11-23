use super::listing::{Listing, Summarized};
use super::one_file::OneFile;
use crate::patch;
use crate::patch::FilePatch;
use client_core::Client;
use gpui_kit::{AppContext as _, Context, EventEmitter, SharedString};
use protocol::model::{ChangeSummary, CommentDraft, CommentId, CommitId, DiffComment, Observed, WorkspaceId};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum ReviewEvent {
    Error(String),
    /// The diff has loaded again.
    Loaded,
    /// The comment being written was saved, which took it off the hub.
    Saved,
    /// The comment being written couldn't be saved, so here it is back, which the hub still keeps.
    NotSaved(CommentDraft),
}

pub struct Review {
    pub(super) client: Client,
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
    /// What the hub's summary of the changed files lists, and the pages read past it, where the hub sends one.
    pub(super) summarized: Option<Summarized>,
    /// How many times the summary has been taken, so a page asked for before it changed isn't listed after it.
    pub(super) pages_asked: u64,
    /// What Changes lists, from the summary or the whole diff, laid out again only when either changes.
    pub(super) listing: Option<Listing>,
    /// The file the file tab shows the diff of, which is read on its own while the summary is current.
    pub(super) diffed: Option<SharedString>,
    /// That file, as one file's diff read it.
    pub(super) one_file: Option<OneFile>,
    /// How many times one file's diff has been asked for, so only the newest answer is taken.
    pub(super) one_file_asks: u64,
    /// The hub, or its worker, refused one file's diff, so the whole diff is read for the file tab.
    pub(super) one_file_refused: bool,
    /// Whether the summary listed is one the hub keeps current now, so nothing reads the whole diff.
    pub(super) summary_current: bool,
}

impl EventEmitter<ReviewEvent> for Review {}

impl Review {
    /// Lists the workspace's changes from `changes`, client-core's summary of them, where it's current, and otherwise
    /// starts loading the whole diff, with the comments client-core holds for the workspace now.
    pub fn new(
        client: Client,
        workspace_id: WorkspaceId,
        held: Vec<DiffComment>,
        changes: Option<&Observed<ChangeSummary>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            client,
            workspace_id,
            files: None,
            base_commit: None,
            truncated: false,
            asks: 0,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
            summarized: None,
            pages_asked: 0,
            listing: None,
            diffed: None,
            one_file: None,
            one_file_asks: 0,
            one_file_refused: false,
            summary_current: false,
        };
        this.set_comments(held, cx);
        // First, so a summary that's current is listed, and the whole diff isn't read.
        this.take_summary(changes, cx);
        this.refresh(cx);
        this
    }

    /// Carries on on a new connection to the hub. The diff is loaded again on it with the next [`Self::refresh`].
    pub fn reconnect(&mut self, client: &Client) {
        self.client = client.clone();
    }

    /// Loads the diff again, keeping the old one until the new one is ready. Not while the hub keeps a summary of the
    /// changed files current, which Changes lists, and which says when the file the file tab shows is to be read again.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.reads_one_file() {
            return;
        }
        self.asks += 1;
        let (ask, client, id) = (self.asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx.background_spawn(async move { client.workspace_diff(&id) }).await;
            let parsed = match loaded {
                Ok(diff) => {
                    let (truncated, base) = (diff.truncated, diff.base_commit);
                    let parsing = cx.background_spawn(async move {
                        let mut files =
                            patch::parse(&diff.patch).map_err(|e| format!("Couldn't read the diff: {e}"))?;
                        files.sort_by(|a, b| a.path().cmp(b.path()));
                        Ok::<_, String>((files, truncated, base))
                    });
                    parsing.await
                }
                Err(e) => Err(client_core::refusals::couldnt("load the diff", &e)),
            };
            let _ = this.update(cx, |t, cx| {
                if ask != t.asks {
                    return;
                }
                match parsed {
                    Ok((files, truncated, base)) => {
                        t.files = Some(files);
                        // In place of a summary that's no longer current, and the file read with it.
                        if !t.summary_current {
                            t.summarized = None;
                            t.one_file = None;
                        }
                        t.truncated = truncated;
                        t.base_commit = base;
                        t.list_again(cx);
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

    /// How many files the whole diff last read changes, while it's what Changes lists. `None` while the summary is
    /// current, which says itself, and until the whole diff is read.
    pub fn whole_diff_changes(&self) -> Option<usize> {
        self.files.as_ref().filter(|_| self.summarized.is_none()).map(Vec::len)
    }

    /// Whether the changes have been read, and the workspace changes nothing.
    pub fn has_no_changes(&self) -> bool {
        self.listing.as_ref().is_some_and(|listing| listing.changed() == 0)
    }

    /// A file as the diff last loaded has it, or, while the summary is current, as its own diff read it, once it's
    /// the one the file tab shows. `None` once it no longer changes anything, and until it's read.
    pub fn file(&self, path: &str) -> Option<&FilePatch> {
        if self.shows_one_file() {
            let file = self.one_file.as_ref().filter(|file| file.of(path))?;
            return file.read.as_ref();
        }
        self.files.as_ref()?.iter().find(|file| file.path().as_ref() == path)
    }

    /// How many times the whole diff has been asked for, for a test to tell that a moment that reads it didn't.
    #[cfg(any(test, feature = "testing"))]
    pub fn whole_diff_reads(&self) -> u64 {
        self.asks
    }

    /// Whether Changes lists a summary the hub keeps current, rather than the whole diff, for a test to tell which.
    #[cfg(any(test, feature = "testing"))]
    pub fn lists_the_summary(&self) -> bool {
        self.summary_current
    }

    /// A comment that's shown.
    pub fn comment(&self, id: &CommentId) -> Option<&DiffComment> {
        self.comments.iter().find(|c| &c.id == id)
    }
}
