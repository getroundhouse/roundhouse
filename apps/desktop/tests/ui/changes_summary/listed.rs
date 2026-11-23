//! What Changes lists: the hub's summary, the pages past it, or the whole diff.

#![cfg(test)]

use client_core::fake_hub::WorkerReads;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{ChangeSummary, CommitId, Observation, Observed, Rev};

use desktop::review::Paging;

use super::world::{BASE, VERSION, counted, current, every_file, listing, reading_everything, summarized, versioned};
use crate::support::Ui;

/// The list, its count and its totals come from the summary rather than the whole diff, which lacks `vendor/`, and a
/// row at its end says how many files it leaves out, which a click reads, after which there's none left. A page asked
/// for just before the summary changes isn't listed with the next.
#[gpui_kit::test]
fn changes_lists_the_summary_and_reads_the_files_it_leaves_out_when_asked(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff) = summarized(cx, (versioned(every_file(), VERSION), 2), reading_everything(), current);
    ui.wait("the summary's files", |ui| ui.has("file-src/new.rs") && ui.has("changed-files-left-out"));
    assert_eq!(counted(&ui), (4, 16), "every file, and the totals over them, 1 + 3 + 5 + 7 lines added");
    assert!(ui.texts_shown().contains("2 more files"));
    let id = ui.view().selected().cloned().unwrap();
    let held = ui.view().records().workspace_changes(&id).cloned();

    ui.cx.update_entity(&diff.review, |r, cx| {
        r.read_more(cx);
        r.take_summary(held.as_ref(), cx);
    });

    ui.frame();
    assert!(ui.has("changed-files-left-out") && !ui.has("file-vendor/x.js"), "the page of the summary before");
    ui.click("changed-files-left-out");
    ui.wait("the files left out", |ui| ui.has("file-vendor/x.js") && ui.has("file-vendor/y.js"));
    assert!(!ui.has("changed-files-left-out"), "none left out");
    assert!(ui.has("file-a.txt") && ui.has("file-src/new.rs"));
    assert_eq!(counted(&ui), (4, 16), "the same files, all listed");
}

/// A hub whose worker is from before pages refuses the page, so the row says how many are left out, and reads no
/// more, until the next connection, where the hub may be another. A page against another base than the summary's,
/// read after the base moved, isn't listed with it.
#[gpui_kit::test]
fn a_page_the_hub_refuses_or_of_another_base_leaves_the_row_saying_how_many(cx: &mut TestAppContext) {
    let another_base =
        WorkerReads { changed_files: Some((CommitId::from("9e8d7c6"), every_file())), ..WorkerReads::default() };
    for (reads, then) in [(WorkerReads::default(), Paging::Refused), (another_base, Paging::Idle)] {
        let (_hub, mut ui, _) = summarized(cx, (every_file(), 2), reads, current);
        ui.wait("the summary's files", |ui| ui.has("file-src/new.rs"));
        let paging = |ui: &Ui<'_>| listing(ui).and_then(|listing| listing.paging);

        ui.click("changed-files-left-out");

        ui.wait("the page", |ui| paging(ui) == Some(then));
        assert!(ui.has("changed-files-left-out") && !ui.has("file-vendor/x.js"), "{then:?}");
        assert_eq!(listing(&ui).map(|listing| listing.left_out), Some(2));
        let (pane, client) = (ui.pane().unwrap(), ui.view().client().unwrap());
        ui.cx.update_entity(&pane, |p, cx| p.reconnect(&client, cx));
        assert_eq!(paging(&ui), Some(Paging::Idle), "asked for again on the next connection");
    }
}

/// A summary as of an earlier read can be older than the whole diff, which is read again as the tab is switched to, so
/// the whole diff's files are listed, as from a hub that sends no summary, saying it leaves some untracked files out.
/// A summary that's current then lists every file, and says nothing of any left out.
#[gpui_kit::test]
fn a_summary_as_of_an_earlier_read_leaves_changes_to_the_whole_diff(cx: &mut TestAppContext) {
    let as_of = |value| Observed::AsOf { value, as_of_ms: 1 };
    let (_hub, mut ui, diff) = summarized(cx, (every_file(), 2), reading_everything(), as_of);
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
