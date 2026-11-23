use super::listed::{Listed, Summary, SummaryFile, WholeDiff};
use super::listing::{ChangedEntry, Listing};
use crate::patch;
use client_core::Client;
use gpui_kit::{AppContext as _, Context, EventEmitter};
use protocol::model::{CommentDraft, CommentId, DiffComment, WorkspaceId};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum ReviewEvent {
    Error(String),
    /// What's read of the changes has been taken in: the whole diff, or the file the file tab shows.
    Loaded,
    /// What's read now says the workspace changes this many files: the whole diff, as it's read, or a summary the hub
    /// keeps current, as it's taken and at each moment that would otherwise read the changes again, so the sidebar's
    /// tabs go by either alike.
    Counted(usize),
    /// The comment being written was saved, which took it off the hub.
    Saved,
    /// The comment being written couldn't be saved, so here it is back, which the hub still keeps.
    NotSaved(CommentDraft),
}

pub struct Review {
    pub(super) client: Client,
    pub(super) workspace_id: WorkspaceId,
    /// What Changes lists, and where the file the file tab shows is read from with it. `None` until the changes are
    /// first read.
    pub(super) listed: Option<Listed>,
    /// What Changes lists, laid out from `listed` again only when that changes.
    pub(super) listing: Option<Listing>,
    /// How many times the whole diff has been asked for, so only the newest answer is taken, and an older one that comes
    /// late isn't shown over it, or taken for what the workspace changes now.
    pub(super) asks: u64,
    /// The file the file tab shows the diff of, which is read on its own while the summary is current.
    pub(super) diffed: Option<ChangedEntry>,
    /// How many times one file's diff has been asked for, so only the newest answer is taken.
    pub(super) one_file_asks: u64,
    /// What's shown: client-core's copy, less what's being deleted here.
    pub comments: Vec<DiffComment>,
    /// Client-core's copy.
    pub(super) held: Vec<DiffComment>,
    /// Deleted here, and still in client-core's copy until the feed says they've gone.
    pub(super) deleting: HashSet<CommentId>,
}

impl EventEmitter<ReviewEvent> for Review {}

impl Review {
    /// With the comments client-core holds for the workspace now, and nothing read: [`Self::take_summary`] lists what
    /// client-core holds of the changed files, and [`Self::refresh`] reads what that leaves to the whole diff.
    pub fn new(client: Client, workspace_id: WorkspaceId, held: Vec<DiffComment>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            client,
            workspace_id,
            listed: None,
            listing: None,
            asks: 0,
            diffed: None,
            one_file_asks: 0,
            comments: Vec::new(),
            held: Vec::new(),
            deleting: HashSet::new(),
        };
        this.set_comments(held, cx);
        this
    }

    /// Carries on on a new connection to the hub, where what the hub or its worker refused before is asked for again,
    /// since the hub may be another now, as `docs/principles/protocol.md` has a client do, or have replaced the worker
    /// that refused. Nothing is read until the next [`Self::refresh`], or the next summary, so a reconnect doesn't
    /// start a worker for every pane at once.
    pub fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        if let Some(Listed::Summary(summary)) = &mut self.listed {
            summary.ask_again();
        }
        self.list_again(cx);
    }

    /// Loads the diff again, keeping the old one until the new one is ready. Not while the hub keeps a summary of the
    /// changed files current, which Changes lists, and which says how many files the workspace changes as it is, and
    /// when the file the file tab shows is to be read again: then only that file is read, if its last read failed.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(Listed::Summary(Summary { current: true, file: SummaryFile::OneFile(_), .. })) = &self.listed {
            self.say_what_changes(cx);
            self.read_the_file_again_if_it_moved(cx);
            return;
        }
        self.asks += 1;
        let (ask, client, id) = (self.asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx.background_spawn(async move { client.workspace_diff(&id) }).await;
            let parsed = match loaded {
                Ok(diff) => {
                    let (truncated, base_commit) = (diff.truncated, diff.base_commit);
                    let parsing = cx.background_spawn(async move {
                        let mut files =
                            patch::parse(&diff.patch).map_err(|e| format!("Couldn't read the diff: {e}"))?;
                        files.sort_by(|a, b| a.path().cmp(b.path()));
                        Ok::<_, String>(WholeDiff { files, base_commit, truncated })
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
                    Ok(whole) => t.took_whole_diff(whole, cx),
                    Err(message) => cx.emit(ReviewEvent::Error(message)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Takes in the whole diff: for the file tab alone while a current summary is listed, from a hub that refused one
    /// file's diff, and otherwise as what Changes lists, in place of a summary that's no longer current, and the file
    /// read with it.
    fn took_whole_diff(&mut self, whole: WholeDiff, cx: &mut Context<Self>) {
        match &mut self.listed {
            Some(Listed::Summary(Summary { current: true, file, .. })) => match file {
                SummaryFile::WholeDiff(read) => *read = Some(whole),
                // Asked for before the summary came, which lists the files now, and reads the file the tab shows.
                SummaryFile::OneFile(_) => return,
            },
            Some(Listed::Summary(Summary { current: false, .. }) | Listed::WholeDiff(_)) | None => {
                self.listed = Some(Listed::WholeDiff(whole));
            }
        }
        self.list_again(cx);
        self.say_what_changes(cx);
        cx.emit(ReviewEvent::Loaded);
    }

    /// Tells whoever listens how many files the workspace changes, where what's listed says so as it is now: the whole
    /// diff, or a summary the hub keeps current. Not one that stopped being current, which may be older than the
    /// changes are.
    pub(super) fn say_what_changes(&self, cx: &mut Context<Self>) {
        let says = match &self.listed {
            Some(Listed::WholeDiff(_) | Listed::Summary(Summary { current: true, .. })) => {
                self.listing.as_ref().map(Listing::changed)
            }
            Some(Listed::Summary(Summary { current: false, .. })) | None => None,
        };
        if let Some(changed) = says {
            cx.emit(ReviewEvent::Counted(changed));
        }
    }

    /// The connection the diff is read on, and its workspace, for reading what each file it changes holds.
    pub(crate) fn source(&self) -> (Client, WorkspaceId) {
        (self.client.clone(), self.workspace_id.clone())
    }

    /// Whether the changes have been read, and the workspace changes nothing.
    pub fn has_no_changes(&self) -> bool {
        self.listing.as_ref().is_some_and(|listing| listing.changed() == 0)
    }

    /// How many times the whole diff has been asked for, for a test to tell that a moment that reads it didn't.
    #[cfg(any(test, feature = "testing"))]
    pub fn whole_diff_reads(&self) -> u64 {
        self.asks
    }

    /// Whether Changes lists a summary the hub keeps current, rather than the whole diff, for a test to tell which.
    #[cfg(any(test, feature = "testing"))]
    pub fn lists_the_summary(&self) -> bool {
        matches!(self.listed, Some(Listed::Summary(Summary { current: true, .. })))
    }

    /// A comment that's shown.
    pub fn comment(&self, id: &CommentId) -> Option<&DiffComment> {
        self.comments.iter().find(|c| &c.id == id)
    }
}
