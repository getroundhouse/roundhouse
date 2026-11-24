//! What Changes lists: the hub's summary, whether current, as of an earlier read, or not known.

#![cfg(test)]

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{ChangeSummary, CommitId, Observation, Observed, Rev};

use super::world::{BASE, VERSION, counted, current, every_file, listing, reading_a_txt, summarized, versioned};
use crate::support::Ui;

/// The list, its count and its totals come from the summary, and a row at its end says how many files it leaves out, which a click doesn't read. A new summary replaces the list, and
/// the row says how many that one leaves out.
#[gpui_kit::test]
fn changes_lists_the_summary_and_says_how_many_files_it_leaves_out(cx: &mut TestAppContext) {
    let (_hub, mut ui, _diff, _asked) = summarized(cx, (versioned(every_file(), VERSION), 2), reading_a_txt(), current);
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

/// A summary as of an earlier read, as while no worker runs, is listed as read, unmarked, as a current one is, and
/// nothing is read as Changes shows it: neither the whole diff nor any file's.
#[gpui_kit::test]
fn a_summary_as_of_an_earlier_read_is_listed_as_read_and_nothing_is_read(cx: &mut TestAppContext) {
    let as_of = |value| Observed::AsOf { value, as_of_ms: 1 };
    let (_hub, mut ui, _diff, asked) = summarized(cx, (every_file(), 2), reading_a_txt(), as_of);

    ui.wait("the summary's files", |ui| ui.has("file-src/new.rs") && ui.has("changed-files-left-out"));
    ui.click("sidebar-tab-changes");

    assert_eq!(counted(&ui), (4, 16), "every file the summary counts, and its totals");
    assert!(ui.texts_shown().contains("2 more files not shown"));
    assert_eq!((asked.count("workspace_diff"), asked.count("workspace_file_diff")), (0, 0), "nothing read");
}

/// A summary that isn't known, as for a workspace whose worker hasn't run since the hub's cache was emptied, lists
/// nothing, as a workspace that changes nothing does, and nothing is read as Changes shows it. A summary that comes
/// then is listed.
#[gpui_kit::test]
fn a_summary_that_isnt_known_lists_nothing_until_one_comes(cx: &mut TestAppContext) {
    let not_known = |_| Observed::NotKnown;
    let (_hub, mut ui, diff, asked) = summarized(cx, (every_file(), 2), reading_a_txt(), not_known);

    ui.wait("the list", |ui| listing(ui).is_some());
    ui.click("sidebar-tab-changes");

    assert_eq!(counted(&ui), (0, 0), "nothing listed");
    assert_eq!((asked.count("workspace_diff"), asked.count("workspace_file_diff")), (0, 0), "nothing read");
    let workspace_id = ui.view().selected().cloned().unwrap();
    let changes = current(ChangeSummary::new(CommitId::from(BASE), every_file()));
    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id,
        changes,
        rev: Rev::new(3),
    });
    ui.wait("the summary's files", |ui| ui.has("file-vendor/y.js"));
    assert_eq!(counted(&ui), (4, 16));
    assert!(ui.cx.read_entity(&diff.review, |r, _| r.listing().is_some()));
}
