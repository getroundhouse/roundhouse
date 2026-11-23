//! The sidebar of changes drawn from the hub's summary of the workspace's changed files, which a hub sends to a client
//! that follows the workspace, with a row for those it leaves out, which reads them a page at a time. On a fake hub,
//! whose world holds the summary, since what's drawn of it is the app's.

#![cfg(test)]

use client_core::fake_hub::{WorkerReads, World};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, CommitId, FileDiff, FileDiffBody, FileVersion, GitDiff, LineCounts,
    Observation, Observed, Rev,
};

use desktop::review::{Listing, Paging};

use crate::support::{Diff, FakeHub, Ui};

/// What git says of the change, which is the whole diff, and which only `a.txt` of the summary is in.
const PATCH: &str = "diff --git a/a.txt b/a.txt
index 5626abf..814f4a4 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1,2 @@
 one
+two
";

fn file(path: &str, status: ChangeStatus, added: u64) -> ChangedFile {
    ChangedFile { path: path.into(), from: None, status, lines: Some(LineCounts { added, removed: 0 }), version: None }
}

/// Every file the workspace changes: two it lists, and two past them, which the hub reads as a page.
fn every_file() -> Vec<ChangedFile> {
    vec![
        file("a.txt", ChangeStatus::Modified, 1),
        file("src/new.rs", ChangeStatus::Untracked, 3),
        file("vendor/x.js", ChangeStatus::Untracked, 5),
        file("vendor/y.js", ChangeStatus::Untracked, 7),
    ]
}

/// What a worker reads on request of [`every_file`]: the pages of those past the summary, and `a.txt`'s diff, in two
/// pages, of [`VERSION`].
fn reading_everything() -> WorkerReads {
    let mut reads = WorkerReads { changed_files: Some((CommitId::from(BASE), every_file())), ..WorkerReads::default() };
    let (head, hunk) = PATCH.split_at(PATCH.find("@@").unwrap());
    let page = |patch: &str, next_line| FileDiff {
        base_commit: CommitId::from(BASE),
        version: Some(FileVersion::from(VERSION.to_string())),
        body: FileDiffBody::Patch { patch: patch.to_string(), next_line },
    };
    reads.file_diffs.insert(("a.txt".into(), 0), page(head, Some(4)));
    reads.file_diffs.insert(("a.txt".into(), 4), page(hunk, None));
    reads
}

/// The commit the changes are against.
const BASE: &str = "3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5";

/// The version of `a.txt` the summary names first.
const VERSION: &str = "5626abf";

/// A workspace whose summary lists the first two files and leaves out two, with its totals over all four, on a fake
/// hub whose worker reads what `reads` says on request, and is otherwise from before the requests, and the diff
/// Changes lists.
fn summarized(
    cx: &mut TestAppContext,
    reads: WorkerReads,
    said: fn(ChangeSummary) -> Observed<ChangeSummary>,
) -> (FakeHub, Ui<'_>, Diff) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, protocol::model::SessionStatus::Idle, Vec::new());
    let base = CommitId::from(BASE);
    let summary = ChangeSummary::listing_at_most(base.clone(), versioned(every_file(), VERSION), 2);
    let changes = said(summary);
    world.observations.push(Observation::WorkspaceChanges {
        workspace_id: workspace.clone(),
        changes,
        rev: Rev::new(2),
    });
    world
        .diffs
        .insert(workspace.clone(), GitDiff { base_commit: Some(base.clone()), patch: PATCH.into(), truncated: false });
    world.worker_reads.insert(workspace.clone(), reads);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    let diff = ui.open_changes();
    (fake, ui, diff)
}

/// `files`, with `a.txt` at `version`.
fn versioned(mut files: Vec<ChangedFile>, version: &str) -> Vec<ChangedFile> {
    files[0].version = Some(FileVersion::from(version.to_string()));
    files
}

fn current(value: ChangeSummary) -> Observed<ChangeSummary> {
    Observed::Current { value }
}

/// What Changes lists of the workspace's changes, once they're read.
fn listing(ui: &Ui<'_>) -> Option<Listing> {
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone()))?;
    ui.cx.read_entity(&review, |r, _| r.listing().cloned())
}

/// How many files the tab over the list counts, and the lines Changes says the workspace adds.
fn counted(ui: &Ui<'_>) -> (usize, usize) {
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone())).unwrap();
    let added = ui.cx.read_entity(&review, |r, _| r.totals().0);
    (listing(ui).map_or(0, |listing| listing.changed()), added)
}

/// The list, its count and its totals come from the summary rather than the whole diff, which lacks `src/new.rs`, and
/// a row at its end says how many files it leaves out, which a click reads, after which there's none left.
#[gpui_kit::test]
fn changes_lists_the_summary_and_reads_the_files_it_leaves_out_when_asked(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = summarized(cx, reading_everything(), current);
    ui.wait("the summary's files", |ui| ui.has("file-src/new.rs"));
    assert_eq!(counted(&ui), (4, 16), "every file, and the totals over them, 1 + 3 + 5 + 7 lines added");
    assert!(ui.texts_shown().contains("2 more files"));

    ui.click("changed-files-left-out");

    ui.wait("the files left out", |ui| ui.has("file-vendor/x.js") && ui.has("file-vendor/y.js"));
    assert!(!ui.has("changed-files-left-out"), "none left out");
    assert!(ui.has("file-a.txt") && ui.has("file-src/new.rs"));
    assert_eq!(counted(&ui), (4, 16), "the same files, all listed");
}

/// A hub whose worker is from before pages refuses the page, so the row says how many are left out, and reads no more.
#[gpui_kit::test]
fn a_hub_that_cant_read_more_leaves_the_row_saying_how_many(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = summarized(cx, WorkerReads::default(), current);
    ui.wait("the summary's files", |ui| ui.has("file-src/new.rs"));

    ui.click("changed-files-left-out");

    let refused = |ui: &mut Ui<'_>| listing(ui).and_then(|listing| listing.paging) == Some(Paging::Refused);
    ui.wait("the refusal", refused);
    assert!(ui.has("changed-files-left-out") && !ui.has("file-vendor/x.js"));
    assert_eq!(listing(&ui).map(|listing| listing.left_out), Some(2));
}

/// A summary as of an earlier read can be older than the whole diff, which is read again as the tab is switched to, so
/// the whole diff's files are listed, as from a hub that sends no summary.
#[gpui_kit::test]
fn a_summary_as_of_an_earlier_read_leaves_changes_to_the_whole_diff(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = summarized(cx, reading_everything(), |value| Observed::AsOf { value, as_of_ms: 1 });

    ui.wait("the whole diff's file", |ui| ui.has("file-a.txt"));

    assert!(!ui.has("file-src/new.rs") && !ui.has("changed-files-left-out"));
    assert_eq!(counted(&ui), (1, 1), "the whole diff's one file, and its line");
}

/// While the summary is current, the file the file tab shows is read on its own, every page of it, and the moments
/// that read the whole diff, such as Changes being chosen, don't. It's read again when the summary gives it another
/// version, and not when it gives another file one.
#[gpui_kit::test]
fn the_file_shown_is_read_on_its_own_and_again_only_when_the_summary_moves_it(cx: &mut TestAppContext) {
    let reads = reading_everything();
    let read = reads.diffs_read.clone();
    let (_hub, mut ui, diff) = summarized(cx, reads, current);
    let workspace = ui.view().selected().cloned().unwrap();
    let summary = |files, rev| {
        let changes = current(ChangeSummary::listing_at_most(CommitId::from(BASE), files, 2));
        Observation::WorkspaceChanges { workspace_id: workspace.clone(), changes, rev: Rev::new(rev) }
    };

    ui.open_diff_of(&diff, "a.txt");

    assert_eq!(*read.lock(), ["a.txt", "a.txt"], "both its pages");
    let whole = |ui: &Ui<'_>| ui.cx.read_entity(&diff.review, |r, _| r.whole_diff_reads());
    let before = whole(&ui);
    ui.click("sidebar-tab-changes");
    assert_eq!(whole(&ui), before, "choosing Changes doesn't read the whole diff");
    let mut another_file_moved = versioned(every_file(), VERSION);
    another_file_moved[1].version = Some(FileVersion::from("ed1f00d".to_string()));
    ui.view().feed().unwrap().take_observation(summary(another_file_moved, 3));
    ui.wait("the summary that moved src/new.rs", |ui| listed(ui, "src/new.rs"));
    assert_eq!(read.lock().len(), 2, "not read again for another file's version");
    ui.view().feed().unwrap().take_observation(summary(versioned(every_file(), "814f4a4"), 4));
    ui.wait("a.txt read again", |_| read.lock().len() == 4);
}

/// Whether the app holds `path` at a version other than the first summary's, which [`VERSION`] gives only `a.txt`.
fn listed(ui: &Ui<'_>, path: &str) -> bool {
    let id = ui.view().selected().cloned().unwrap();
    let records = ui.view().records().clone();
    let Some(Observed::Current { value }) = records.workspace_changes(&id) else { return false };
    let version = value.files.iter().find(|file| file.path == path).and_then(|file| file.version.as_ref());
    version.is_some_and(|version| version.as_str() != VERSION)
}

/// A hub whose worker is from before one file's diff refuses it, so the file tab shows the file from the whole diff,
/// as from a hub that keeps no summary.
#[gpui_kit::test]
fn a_hub_that_cant_read_one_file_shows_it_from_the_whole_diff(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff) = summarized(cx, WorkerReads::default(), current);

    ui.open_diff_of(&diff, "a.txt");

    let lines = ui.cx.read_entity(&diff.file, |view, cx| view.file(cx).map(desktop::patch::FilePatch::added));
    assert_eq!(lines, Some(1), "the whole diff's a.txt");
}
