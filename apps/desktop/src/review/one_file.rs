//! The file the file tab shows, read with one file's diff while the hub keeps the workspace's changed files current,
//! and read again only when the summary gives it another version, base, status or old path, so the whole diff is never
//! read for it.

use gpui_kit::{AppContext as _, Context, SharedString};
use protocol::model::{ChangeStatus, CommitId, FileDiff, FileDiffBody, FileVersion};

use super::listing::file_status;
use super::{Review, ReviewEvent};
use crate::patch::{self, FilePatch};

/// How many times a file's pages are read from its start again because a later page was of another version of it,
/// after which what was read shows, since a file written that often is shown again when the summary next moves.
const MOST_READS_AGAIN: usize = 3;

/// What the summary says of a file, which one file's diff of it is read again only when it changes.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    base: CommitId,
    status: ChangeStatus,
    from: Option<String>,
    version: Option<FileVersion>,
}

/// The file the file tab shows, as one file's diff read it.
#[derive(Debug)]
pub(super) struct OneFile {
    path: SharedString,
    /// What the summary said of it when it was asked for, or `None` where it lists it nowhere.
    key: Option<Key>,
    /// Its diff, once read. `None` until it's read, and where the worktree changes nothing at its path now.
    pub(super) read: Option<FilePatch>,
}

impl OneFile {
    pub(super) fn of(&self, path: &str) -> bool {
        self.path.as_ref() == path
    }
}

impl Review {
    /// Says which file the file tab shows the diff of, or none, and reads it, while the summary is current.
    pub fn show_diff_of(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.diffed = path;
        self.read_the_file_again_if_it_moved(cx);
    }

    /// Whether the file tab's file is read with one file's diff now: while the hub keeps a summary current, unless it,
    /// or its worker, is from before the request, which leaves it to the whole diff.
    pub(super) fn reads_one_file(&self) -> bool {
        self.summary_current && !self.one_file_refused
    }

    /// Whether the file tab's file is the one its own diff read: while Changes lists a summary, current or until the
    /// whole diff is read again, unless the hub refused it.
    pub(super) fn shows_one_file(&self) -> bool {
        self.summarized.is_some() && !self.one_file_refused
    }

    /// Reads the file the file tab shows again if the summary says something else of it than when it was last read, or
    /// it's another file, and lets go of the one read once the tab shows none.
    pub(super) fn read_the_file_again_if_it_moved(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.diffed.clone() else {
            self.one_file = None;
            return;
        };
        // As it was read, until the whole diff is read again.
        if !self.reads_one_file() {
            return;
        }
        let key = self.key_of(&path);
        if self.one_file.as_ref().is_some_and(|file| file.of(&path) && file.key == key) {
            return;
        }
        let from = key.as_ref().and_then(|key| key.from.clone());
        self.one_file = Some(OneFile { path: path.clone(), key, read: None });
        self.one_file_asks += 1;
        let (asked, client, id) = (self.one_file_asks, self.client.clone(), self.workspace_id.clone());
        cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { every_page(&client, &id, &path, from.as_deref()) }).await;
            let _ = this.update(cx, |t, cx| t.took_one_file(asked, read, cx));
        })
        .detach();
    }

    /// Takes in what reading the file the tab shows found, unless another read has been asked for since.
    fn took_one_file(&mut self, asked: u64, read: client_core::Result<Option<FileDiff>>, cx: &mut Context<Self>) {
        if asked != self.one_file_asks {
            return;
        }
        let Some(file) = &mut self.one_file else { return };
        match read {
            Ok(diff) => {
                let (status, from) =
                    (file.key.as_ref().map(|key| key.status), file.key.as_ref().and_then(|key| key.from.clone()));
                let base = diff.as_ref().map(|diff| diff.base_commit.clone());
                file.read = diff.and_then(|diff| patched(&file.path, from, status, diff));
                // The commit what the diff leaves out is read from, as the whole diff's is.
                self.base_commit = base.or_else(|| self.base_commit.clone());
            }
            // A hub, or a worker, from before it, so the file is read with the whole diff, as from a hub that keeps
            // no summary.
            Err(e) if e.is_unsupported() => {
                self.one_file_refused = true;
                self.one_file = None;
                self.refresh(cx);
                return;
            }
            Err(e) => {
                cx.emit(ReviewEvent::Error(client_core::refusals::couldnt("load the file's diff", &e)));
                return;
            }
        }
        cx.emit(ReviewEvent::Loaded);
        cx.notify();
    }

    /// What the summary says of the file at `path`.
    fn key_of(&self, path: &str) -> Option<Key> {
        let summarized = self.summarized.as_ref()?;
        let file = summarized.file(path)?;
        Some(Key {
            base: summarized.base_commit.clone(),
            status: file.status,
            from: file.from.clone(),
            version: file.version.clone(),
        })
    }
}

/// Every page of the diff of the file at `path`, renamed from `from`, as one: read from its start again when a later
/// page is of another version of the file than the first, up to [`MOST_READS_AGAIN`] times. `None` where the hub says
/// nothing of it.
fn every_page(
    client: &client_core::Client,
    id: &protocol::model::WorkspaceId,
    path: &str,
    from: Option<&str>,
) -> client_core::Result<Option<FileDiff>> {
    let mut reads = 0;
    'read: loop {
        let first = client.workspace_file_diff(id, path, from, 0)?;
        let FileDiffBody::Patch { patch, mut next_line } = first.body else { return Ok(Some(first)) };
        let mut whole = patch;
        while let Some(line) = next_line {
            let page = client.workspace_file_diff(id, path, from, line)?;
            if page.version != first.version && reads < MOST_READS_AGAIN {
                reads += 1;
                continue 'read;
            }
            let FileDiffBody::Patch { patch, next_line: next } = page.body else { break };
            whole.push_str(&patch);
            next_line = next;
        }
        let body = FileDiffBody::Patch { patch: whole, next_line: None };
        return Ok(Some(FileDiff { body, ..first }));
    }
}

/// The file `diff` is of, as the file tab draws it, moved from `from` and changed as `status` says where the summary
/// lists it. `None`
/// where the worktree changes nothing at `path` now, or the hub sends what this build can't draw.
fn patched(
    path: &SharedString,
    from: Option<String>,
    status: Option<ChangeStatus>,
    diff: FileDiff,
) -> Option<FilePatch> {
    match diff.body {
        FileDiffBody::Patch { patch, .. } => patch::parse(&patch).ok()?.into_iter().next(),
        FileDiffBody::Binary => {
            let status = file_status(status.unwrap_or(ChangeStatus::Modified));
            Some(FilePatch::binary(path.clone(), from.map(SharedString::from), status))
        }
        FileDiffBody::Unchanged | FileDiffBody::Unknown => None,
    }
}
