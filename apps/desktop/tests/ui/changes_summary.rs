//! The sidebar of changes drawn from the hub's summary of the workspace's changed files, which a hub sends to a client
//! that follows the workspace, with a row for those it leaves out, which reads them a page at a time. On a fake hub,
//! whose world holds the summary, since what's drawn of it is the app's.

#![cfg(test)]

use client_core::fake_hub::World;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, CommitId, GitDiff, LineCounts, Observation, Observed, Rev,
};

use desktop::review::{Listing, Paging};

use crate::support::{FakeHub, Ui};

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

/// A workspace whose summary lists the first two files and leaves out two, with its totals over all four, on a fake
/// hub whose worker reads the rest when `pages` says so, and otherwise is from before pages.
fn summarized(
    cx: &mut TestAppContext,
    pages: bool,
    said: fn(ChangeSummary) -> Observed<ChangeSummary>,
) -> (FakeHub, Ui<'_>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, protocol::model::SessionStatus::Idle, Vec::new());
    let base = CommitId::from("3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5");
    let summary = ChangeSummary::listing_at_most(base.clone(), every_file(), 2);
    let changes = said(summary);
    world.observations.push(Observation::WorkspaceChanges {
        workspace_id: workspace.clone(),
        changes,
        rev: Rev::new(2),
    });
    world
        .diffs
        .insert(workspace.clone(), GitDiff { base_commit: Some(base.clone()), patch: PATCH.into(), truncated: false });
    if pages {
        world.changed_files.insert(workspace.clone(), (base, every_file()));
    }
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    ui.open_changes();
    (fake, ui)
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
    let (_hub, mut ui) = summarized(cx, true, current);
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
    let (_hub, mut ui) = summarized(cx, false, current);
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
    let (_hub, mut ui) = summarized(cx, true, |value| Observed::AsOf { value, as_of_ms: 1 });

    ui.wait("the whole diff's file", |ui| ui.has("file-a.txt"));

    assert!(!ui.has("file-src/new.rs") && !ui.has("changed-files-left-out"));
    assert_eq!(counted(&ui), (1, 1), "the whole diff's one file, and its line");
}
