//! What Changes lists: the hub's summary, or the whole diff.

#![cfg(test)]

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{ChangeSummary, CommitId, Observation, Observed, Rev};

use super::world::{BASE, VERSION, counted, current, every_file, listing, reading_a_txt, summarized, versioned};
use crate::support::Ui;

/// The list, its count and its totals come from the summary rather than the whole diff, which lacks `vendor/`, and a
/// row at its end says how many files it leaves out, which a click doesn't read. A new summary replaces the list, and
/// the row says how many that one leaves out.
#[gpui_kit::test]
fn changes_lists_the_summary_and_says_how_many_files_it_leaves_out(cx: &mut TestAppContext) {
    let (_hub, mut ui, _diff) = summarized(cx, (versioned(every_file(), VERSION), 2), reading_a_txt(), current);
    ui.wait("the summary's files", |ui| ui.has("file-src/new.rs") && ui.has("changed-files-left-out"));
    assert_eq!(counted(&ui), (4, 16), "every file, and the totals over them, 1 + 3 + 5 + 7 lines added");
    assert!(ui.texts_shown().contains("2 more files not shown"));
    let rows = |ui: &Ui<'_>| listing(ui).map(|listing| (listing.rows.len(), listing.left_out));

    ui.click("changed-files-left-out");

    assert_eq!(rows(&ui), Some((2, 2)), "nothing read past the summary");
    assert!(ui.has("changed-files-left-out") && !ui.has("file-vendor/x.js"));
    let workspace_id = ui.view().selected().cloned().unwrap();
    let changes = current(ChangeSummary::listing_at_most(CommitId::from(BASE), every_file(), 3));
    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id,
        changes,
        rev: Rev::new(3),
    });
    ui.wait("the next summary's files", |ui| ui.has("file-vendor/x.js"));
    assert!(ui.texts_shown().contains("1 more file not shown"));
    assert!(ui.has("file-a.txt") && !ui.has("file-vendor/y.js"));
    assert_eq!(counted(&ui), (4, 16), "the same files, one more of them listed");
}

/// A summary as of an earlier read can be older than the whole diff, which is read again as the tab is switched to, so
/// the whole diff's files are listed, as from a hub that sends no summary, saying it leaves some untracked files out.
/// A summary that's current then lists every file, and says nothing of any left out.
#[gpui_kit::test]
fn a_summary_as_of_an_earlier_read_leaves_changes_to_the_whole_diff(cx: &mut TestAppContext) {
    let as_of = |value| Observed::AsOf { value, as_of_ms: 1 };
    let (_hub, mut ui, diff) = summarized(cx, (every_file(), 2), reading_a_txt(), as_of);
    let truncated = |ui: &Ui<'_>| listing(ui).is_some_and(|listing| listing.truncated);

    ui.wait("the whole diff's files", |ui| ui.has("file-a.txt") && ui.has("file-src/new.rs"));

    assert!(!ui.has("changed-files-left-out"));
    assert_eq!(counted(&ui), (2, 4), "the whole diff's two files, and their lines");
    assert!(truncated(&ui), "the whole diff left untracked files out");
    let workspace_id = ui.view().selected().cloned().unwrap();
    let changes = current(ChangeSummary::new(CommitId::from(BASE), every_file()));
    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id,
        changes,
        rev: Rev::new(3),
    });
    ui.wait("the summary's files", |ui| ui.cx.read_entity(&diff.review, |r, _| r.lists_the_summary()));
    assert_eq!(counted(&ui), (4, 16));
    assert!(!truncated(&ui), "a summary that leaves nothing out");
}
