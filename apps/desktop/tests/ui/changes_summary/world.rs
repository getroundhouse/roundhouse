//! A workspace whose hub keeps a summary of its changed files, on a fake hub, and what its worker reads on request.

#![cfg(test)]

use client_core::fake_hub::{WorkerReads, World};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, CommitId, FileDiff, FileDiffBody, FileVersion, GitDiff,
    LineCounts, Observation, Observed, Rev,
};

use desktop::review::Listing;

use crate::support::{Diff, FakeHub, Ui};

/// What git says of the change, which is the whole diff: `a.txt` and `src/new.rs` of the summary, with other lines
/// than one file's diff gives `a.txt`, and without `vendor/`, which it leaves out, as it leaves out untracked files
/// past a bound.
const PATCH: &str = "diff --git a/a.txt b/a.txt
index 5626abf..814f4a4 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1,2 @@
 one
+two
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
index 0000000..3b18e51
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,3 @@
+fn a() {}
+fn b() {}
+fn c() {}
";

pub(super) fn file(path: &str, status: ChangeStatus, added: u64) -> ChangedFile {
    ChangedFile { path: path.into(), from: None, status, lines: Some(LineCounts { added, removed: 0 }), version: None }
}

/// Every file the workspace changes: two it lists, and two past them, which it leaves out.
pub(super) fn every_file() -> Vec<ChangedFile> {
    vec![
        file("a.txt", ChangeStatus::Modified, 1),
        file("src/new.rs", ChangeStatus::Untracked, 3),
        file("vendor/x.js", ChangeStatus::Untracked, 5),
        file("vendor/y.js", ChangeStatus::Untracked, 7),
    ]
}

/// The pages of a diff of `path`'s one line with `added` after it, at `version`: its headers, then its hunk.
pub(super) fn pages_of(path: &str, added: &[&str], version: &str) -> [FileDiff; 2] {
    let [head, hunk] = patch_of(path, added);
    [page(&head, version, Some(4)), page(&hunk, version, None)]
}

/// The patch of `path`'s one line with `added` after it: its headers, which are 4 lines, then its hunk.
pub(super) fn patch_of(path: &str, added: &[&str]) -> [String; 2] {
    let head = format!("diff --git a/{path} b/{path}\nindex 5626abf..814f4a4 100644\n--- a/{path}\n+++ b/{path}\n");
    let lines: String = added.iter().map(|line| ["+", line, "\n"].concat()).collect();
    [head, format!("@@ -1 +1,{} @@\n one\n{lines}", added.len() + 1)]
}

/// A page of one file's diff, against [`BASE`], of `version`.
pub(super) fn page(patch: &str, version: &str, next_line: Option<u32>) -> FileDiff {
    let body = FileDiffBody::Patch { patch: patch.to_string(), next_line };
    FileDiff { base_commit: CommitId::from(BASE), version: Some(FileVersion::from(version.to_string())), body }
}

/// A page of one file's diff, against [`BASE`], of `version`, or of none, with `body`.
pub(super) fn page_with(body: FileDiffBody, version: Option<&str>) -> FileDiff {
    FileDiff { base_commit: CommitId::from(BASE), version: version.map(|version| version.to_string().into()), body }
}

/// What a worker reads on request of [`every_file`]: `a.txt`'s diff, in two pages, of [`VERSION`] and then of
/// `814f4a4`, once it's written again, each with a line the whole diff lacks.
pub(super) fn reading_a_txt() -> WorkerReads {
    let mut reads = WorkerReads::default();
    let [head, hunk] = pages_of("a.txt", &["read on its own"], VERSION);
    let [head_again, hunk_again] = pages_of("a.txt", &["read on its own", "and read again"], "814f4a4");
    reads.file_diffs.insert((tracked("a.txt"), 0), vec![head, head_again]);
    reads.file_diffs.insert((tracked("a.txt"), 4), vec![hunk, hunk_again]);
    reads
}

/// The tracked file at `path`, as a summary names it.
pub(super) fn tracked(path: &str) -> ChangedFileAt {
    ChangedFileAt { untracked: false, path: path.to_string() }
}

/// The commit the changes are against.
pub(super) const BASE: &str = "3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5";

/// The version of `a.txt` the summary names first.
pub(super) const VERSION: &str = "5626abf";

/// A workspace whose summary lists the first `most` of `files` and leaves out the rest, with its totals over all of
/// them, on a fake hub whose worker reads what `reads` says on request, and is otherwise from before the requests, and
/// the diff Changes lists.
pub(super) fn summarized(
    cx: &mut TestAppContext,
    (files, most): (Vec<ChangedFile>, usize),
    reads: WorkerReads,
    said: fn(ChangeSummary) -> Observed<ChangeSummary>,
) -> (FakeHub, Ui<'_>, Diff) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, protocol::model::SessionStatus::Idle, Vec::new());
    let base = CommitId::from(BASE);
    let changes = said(ChangeSummary::listing_at_most(base.clone(), files, most));
    world.observations.push(Observation::WorkspaceChanges {
        workspace_id: workspace.clone(),
        changes,
        rev: Rev::new(2),
    });
    world.diffs.insert(workspace.clone(), GitDiff { base_commit: Some(base), patch: PATCH.into(), truncated: true });
    world.worker_reads.insert(workspace.clone(), reads);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    let diff = ui.open_changes();
    (fake, ui, diff)
}

/// `files`, with `a.txt` at `version`.
pub(super) fn versioned(mut files: Vec<ChangedFile>, version: &str) -> Vec<ChangedFile> {
    files[0].version = Some(FileVersion::from(version.to_string()));
    files
}

pub(super) fn current(value: ChangeSummary) -> Observed<ChangeSummary> {
    Observed::Current { value }
}

/// What Changes lists of the workspace's changes, once they're read.
pub(super) fn listing(ui: &Ui<'_>) -> Option<Listing> {
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone()))?;
    ui.cx.read_entity(&review, |r, _| r.listing().cloned())
}

/// How many files the tab over the list counts, and the lines Changes says the workspace adds.
pub(super) fn counted(ui: &Ui<'_>) -> (usize, usize) {
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone())).unwrap();
    let added = ui.cx.read_entity(&review, |r, _| r.totals().0);
    (listing(ui).map_or(0, |listing| listing.changed()), added)
}

/// The text of line `new` of the new side of the file the file tab shows, if it draws it.
pub(super) fn drawn_line(ui: &Ui<'_>, diff: &Diff, new: u32) -> Option<String> {
    ui.cx.read_entity(&diff.file, |view, _| view.shows_line(new).map(|line| line.to_string()))
}
