//! The file the file tab shows, read with one file's diff while the summary is current.

#![cfg(test)]

use std::sync::Arc;

use client_core::fake_hub::WorkerReads;
use gpui_kit::{AppContext as _, TestAppContext};
use parking_lot::Mutex;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, CommitId, FileDiff, FileDiffBody, FileVersion,
    Observation, Observed, Record, Rev, Workspace, WorkspaceStatus,
};

use desktop::patch::FilePatch;
use desktop::review::Diffed;

use super::world::{
    BASE, VERSION, current, drawn_line, every_file, file, listing, page_with, pages_of, patch_of, reading_a_txt,
    summarized, tracked, versioned,
};
use crate::support::{Diff, Ui};

/// While the summary is current, the file the file tab shows is read on its own, every page of it, and the moments
/// that read the whole diff, such as Changes being chosen, don't. It's read again when the summary gives it another
/// version, showing what was read until the new read comes, and not when it gives another file one. Once the summary
/// stops being current, it stays listed, and another file clicked is read with the whole diff, which takes over, and
/// once a summary is current again, that file is read on its own as the summary names it.
#[gpui_kit::test]
fn the_file_shown_is_read_on_its_own_and_again_only_when_the_summary_moves_it(cx: &mut TestAppContext) {
    let reads = reading_a_txt();
    let read = reads.diffs_read.clone();
    let (_hub, mut ui, diff) = summarized(cx, (versioned(every_file(), VERSION), 2), reads, current);
    let workspace = ui.view().selected().cloned().unwrap();
    let summary = |files, rev| {
        let changes = current(ChangeSummary::listing_at_most(CommitId::from(BASE), files, 2));
        Observation::WorkspaceChanges { workspace_id: workspace.clone(), changes, rev: Rev::new(rev) }
    };
    let pages_read = |read: &Arc<Mutex<Vec<(ChangedFileAt, u32)>>>| -> Vec<u32> {
        read.lock().iter().map(|(_, line)| *line).collect()
    };
    let whole = |ui: &Ui<'_>| ui.cx.read_entity(&diff.review, |r, _| r.whole_diff_reads());

    ui.open_diff_of(&diff, "a.txt");

    assert_eq!(pages_read(&read), [0, 4], "both its pages");
    assert_eq!(drawn_line(&ui, &diff, 2).as_deref(), Some("read on its own"), "its own diff, not the whole diff's");
    let before = whole(&ui);
    ui.click("sidebar-tab-changes");
    assert_eq!(whole(&ui), before, "choosing Changes doesn't read the whole diff");
    let mut another_file_moved = versioned(every_file(), VERSION);
    another_file_moved[1].version = Some(FileVersion::from("ed1f00d".to_string()));
    ui.view().feed().unwrap().take_observation(summary(another_file_moved, 3));
    ui.wait("the summary that moved src/new.rs", |ui| listed(ui, "src/new.rs"));
    assert_eq!(read.lock().len(), 2, "not read again for another file's version");
    let moved = ChangeSummary::listing_at_most(CommitId::from(BASE), versioned(every_file(), "814f4a4"), 2);
    ui.cx.update_entity(&diff.review, |r, cx| r.take_summary(Some(&current(moved.clone())), cx));
    let (says, drawn) =
        ui.cx.read_entity(&diff.file, |view, cx| (view.says_instead(cx), view.file(cx).map(FilePatch::added)));
    assert_eq!((says, drawn), (None, Some(1)), "what was read, while it's read again");
    ui.wait("a.txt read again", |ui| drawn_line(ui, &diff, 3).as_deref() == Some("and read again"));
    assert_eq!(pages_read(&read), [0, 4, 0, 4]);

    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id: workspace.clone(),
        changes: Observed::AsOf { value: moved, as_of_ms: 1 },
        rev: Rev::new(5),
    });
    let lists_the_summary = |ui: &Ui<'_>| ui.cx.read_entity(&diff.review, |r, _| r.lists_the_summary());
    ui.wait("the summary as of a read", |ui| !lists_the_summary(ui));
    assert!(ui.has("changed-files-left-out"), "the summary stays listed once it's no longer current");
    assert_eq!(whole(&ui), before, "and nothing reads the whole diff for it");
    ui.open_diff_of(&diff, "src/new.rs");
    assert_eq!(whole(&ui), before + 1, "another file is read with the whole diff");
    assert_eq!(drawn_line(&ui, &diff, 3).as_deref(), Some("fn c() {}"));
    assert!(!ui.has("changed-files-left-out"), "which Changes lists now");
    // Clicked from the whole diff, which can't say the file is untracked, it's asked for as the next summary names it.
    ui.click("file-src/new.rs");
    ui.view().feed().unwrap().take_observation(summary(versioned(every_file(), "814f4a4"), 6));
    let asked = |read: &Arc<Mutex<Vec<(ChangedFileAt, u32)>>>| read.lock().last().map(|(at, _)| at.clone());
    ui.wait("src/new.rs read on its own", |_| asked(&read).is_some_and(|at| at.path == "src/new.rs"));
    assert_eq!(asked(&read).map(|at| at.untracked), Some(true), "as the summary names it");
}

/// Once the workspace is being archived, the hub refuses every read of its worktree, so nothing is read: the summary
/// that's no longer current stays listed, the file read before shows as it was read, and another file clicked is read
/// neither on its own nor with the whole diff, and says it couldn't be read, with no error.
#[gpui_kit::test]
fn nothing_is_read_of_a_workspace_being_archived(cx: &mut TestAppContext) {
    let reads = reading_a_txt();
    let read = reads.diffs_read.clone();
    let (_hub, mut ui, diff) = summarized(cx, (versioned(every_file(), VERSION), 2), reads, current);
    let workspace = ui.view().selected().cloned().unwrap();
    let whole = |ui: &Ui<'_>| ui.cx.read_entity(&diff.review, |r, _| r.whole_diff_reads());
    ui.open_diff_of(&diff, "a.txt");
    let before = (whole(&ui), read.lock().len());
    let archiving = Workspace {
        status: WorkspaceStatus::Archiving,
        rev: Rev::new(10),
        ..ui.view().records().workspace(&workspace).unwrap().clone()
    };
    let as_of = ChangeSummary::listing_at_most(CommitId::from(BASE), versioned(every_file(), VERSION), 2);

    ui.view().feed().unwrap().take_record(Record::Workspace(archiving));
    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id: workspace.clone(),
        changes: Observed::AsOf { value: as_of, as_of_ms: 1 },
        rev: Rev::new(11),
    });
    ui.wait("the summary as of a read", |ui| !ui.cx.read_entity(&diff.review, |r, _| r.lists_the_summary()));
    ui.click("file-src/new.rs");
    ui.wait("src/new.rs to say it couldn't be read", |ui| {
        ui.cx.read_entity(&diff.file, desktop::file_diff::FileDiff::says_instead) == Some(COULDNT)
    });

    assert_eq!((whole(&ui), read.lock().len()), before, "nothing read");
    assert_eq!(ui.view().error.as_deref(), None);
    assert!(ui.has("changed-files-left-out"), "the summary stays listed");
    ui.click("file-a.txt");
    ui.wait("a.txt as it was read", |ui| drawn_line(ui, &diff, 2).as_deref() == Some("read on its own"));
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
/// as from a hub that keeps no summary, until the next connection, where it's asked for again.
#[gpui_kit::test]
fn a_hub_that_cant_read_one_file_shows_it_from_the_whole_diff(cx: &mut TestAppContext) {
    let reads = WorkerReads::default();
    let asked = reads.diffs_read.clone();
    let (_hub, mut ui, diff) = summarized(cx, (every_file(), 2), reads, current);
    let added = |ui: &Ui<'_>| ui.cx.read_entity(&diff.file, |view, cx| view.file(cx).map(FilePatch::added));

    ui.open_diff_of(&diff, "a.txt");

    assert_eq!(added(&ui), Some(1), "the whole diff's a.txt");
    assert_eq!(asked.lock().len(), 1, "refused");
    let (pane, client) = (ui.pane().unwrap(), ui.view().client().unwrap());
    ui.cx.update_entity(&pane, |p, cx| {
        p.reconnect(&client, cx);
        p.catch_up(cx);
    });
    ui.wait("the file's diff asked for again", |_| asked.lock().len() == 2);
    ui.wait("the whole diff's a.txt again", |ui| added(ui) == Some(1));
}

/// What the file tab shows of a file, once its own diff is read.
#[derive(Clone, Copy, Debug)]
enum Shows {
    /// Its diff, with this as line 2 of the new side, which is the line it adds.
    Adds(&'static str),
    /// The comparison of an image's two sides, which a binary file shows in place of lines.
    Image,
    /// What it says in place of a diff.
    Says(&'static str),
}

/// A file of [`the_file_tab_draws_what_one_file_s_diff_says_of_the_file`], which the summary names `v2` of.
struct Row {
    path: &'static str,
    /// How the worker reads each page of its diff, by the line the page starts at, each time it's asked for it.
    reads: Vec<(u32, Vec<FileDiff>)>,
    shows: Shows,
    /// How many pages of it are read.
    pages: usize,
}

fn read_and_shown() -> Vec<Row> {
    let row = |path, reads, shows, pages| Row { path, reads, shows, pages };
    let pages = |path, added, version: Option<&str>| {
        let [head, hunk] = pages_of(path, &[added], version.unwrap_or_default());
        [head, hunk].map(|page| FileDiff { version: version.map(|version| version.to_string().into()), ..page })
    };
    let [first_head, _] = pages("another-version.txt", "of the first version", Some("v1"));
    let [second_head, second_hunk] = pages("another-version.txt", "of the second version", Some("v2"));
    let [cut_head, cut_hunk] = pages("cut-off.txt", "whole", Some("v2"));
    let [neither_head, neither_hunk] = pages("unversioned.txt", "of neither version", None);
    let [listed_head, listed_hunk] = pages("unversioned.txt", "of the listed version", Some("v2"));
    let [changing_head, _] = pages("kept-changing.txt", "of one version", Some("v1"));
    let [_, changing_hunk] = pages("kept-changing.txt", "of another", Some("v2"));
    let unchanged = page_with(FileDiffBody::Unchanged, Some("v2"));
    let unreadable = FileDiffBody::Patch { patch: "@@ -1 +1,2 @@\n one\n+two\n".into(), next_line: None };
    let unreadable = page_with(unreadable, Some("v2"));
    let read_at_last = patch_of("unreadable.txt", &["read at last"]).concat();
    let read_at_last = page_with(FileDiffBody::Patch { patch: read_at_last, next_line: None }, Some("v2"));
    let second = Shows::Adds("of the second version");
    vec![
        // A later page of another version than the first.
        row("another-version.txt", vec![(0, vec![first_head, second_head]), (4, vec![second_hunk])], second, 4),
        // A later page that isn't a patch.
        row("cut-off.txt", vec![(0, vec![cut_head]), (4, vec![unchanged.clone(), cut_hunk])], Shows::Adds("whole"), 4),
        row("put-back.txt", vec![(0, vec![unchanged])], Shows::Says("This file has no changes now"), 1),
        // Two pages that name no version, of a file the summary names one of.
        row(
            "unversioned.txt",
            vec![(0, vec![neither_head, listed_head]), (4, vec![neither_hunk, listed_hunk])],
            Shows::Adds("of the listed version"),
            4,
        ),
        row("shot.png", vec![(0, vec![page_with(FileDiffBody::Binary, Some("v2"))])], Shows::Image, 1),
        // Pages that are never of one version, which are read again from the first, and then no more.
        row("kept-changing.txt", vec![(0, vec![changing_head]), (4, vec![changing_hunk])], Shows::Says(COULDNT), 8),
        // Marked, as its name has a byte replaced, so it's never asked for.
        row(NOT_UTF8, Vec::new(), Shows::Says("This file's name isn't valid UTF-8, so its changes can't be shown"), 0),
        // Read again at the next summary, and at the next connection, after which it can be read.
        row("unreadable.txt", vec![(0, vec![unreadable.clone(), unreadable, read_at_last])], Shows::Says(COULDNT), 1),
    ]
}

/// What the file tab says of a file it couldn't read.
const COULDNT: &str = "Couldn't read this file's changes";

/// The name of a file whose name has a byte that isn't UTF-8, which the worker replaces, and marks the file.
const NOT_UTF8: &str = "caf\u{fffd}.txt";

/// Opens the file at `path` in Changes, and waits for the file tab to show it as `shows` says.
fn open_and_see(ui: &mut Ui<'_>, diff: &Diff, path: &str, shows: Shows) {
    let row = gpui_kit::SharedString::from(format!("file-{path}"));
    ui.wait("its row", |ui| ui.has(row.clone()));
    ui.click(row);
    see(ui, diff, path, shows);
}

/// Waits for the file tab to show the file at `path` as `shows` says.
fn see(ui: &mut Ui<'_>, diff: &Diff, path: &str, shows: Shows) {
    let drawn = |ui: &Ui<'_>| {
        ui.cx.read_entity(&diff.file, |view, cx| {
            let file = view.file(cx).filter(|file| file.path() == path).map(FilePatch::is_binary);
            (file, view.says_instead(cx))
        })
    };
    let read_now = |ui: &Ui<'_>| {
        let tab = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.file_tab());
        tab.as_deref() == Some(path) && ui.cx.read_entity(&diff.review, |r, _| r.diffed() != Diffed::Loading)
    };
    ui.wait(path, |ui| match shows {
        Shows::Adds(line) => drawn(ui) == (Some(false), None) && drawn_line(ui, diff, 2).as_deref() == Some(line),
        Shows::Image => drawn(ui) == (Some(true), None),
        Shows::Says(said) => read_now(ui) && drawn(ui).1 == Some(said),
    });
}

/// What the file tab draws of what one file's diff says of each kind of file: an image git takes as binary as the
/// image's comparison, and a file the worktree no longer changes, or whose patch can't be read, as that. Its pages are
/// joined only where each is a patch of the version the first is of, and otherwise read again from the first, but for
/// a file that keeps changing as it's read. One whose read fails is read again with the next summary, and on the next
/// connection. One the summary marks as named in bytes that aren't UTF-8 says why it shows no diff, and nothing is
/// read for it, while its row keeps its lines.
#[gpui_kit::test]
fn the_file_tab_draws_what_one_file_s_diff_says_of_the_file(cx: &mut TestAppContext) {
    let rows = read_and_shown();
    let mut reads = WorkerReads::default();
    for Row { path, reads: pages, .. } in &rows {
        for (line, read) in pages {
            reads.file_diffs.insert((tracked(path), *line), read.clone());
        }
    }
    let read = reads.diffs_read.clone();
    let reads_of = |path: &str| read.lock().iter().filter(|(read, _)| read.path == path).count();
    let listed = |path: &&str| ChangedFile {
        version: Some("v2".to_string().into()),
        name_not_utf8: *path == NOT_UTF8,
        ..file(path, ChangeStatus::Modified, 1)
    };
    let files: Vec<_> = rows.iter().map(|row| listed(&row.path)).collect();
    let (_hub, mut ui, diff) = summarized(cx, (files.clone(), rows.len()), reads, current);
    let whole_diff_reads = ui.cx.read_entity(&diff.review, |r, _| r.whole_diff_reads());

    for Row { path, shows, pages, .. } in rows {
        open_and_see(&mut ui, &diff, path, shows);

        assert_eq!(reads_of(path), pages, "{path}'s pages read");
    }
    assert_eq!(ui.cx.read_entity(&diff.review, |r, _| r.whole_diff_reads()), whole_diff_reads, "nor the whole diff");
    let not_utf8 = listing(&ui).and_then(|listing| listing.rows.iter().find(|row| row.path == NOT_UTF8).cloned());
    assert_eq!(not_utf8.map(|row| (row.added, row.removed)), Some((1, 0)), "{NOT_UTF8}'s row keeps its lines");

    let workspace_id = ui.view().selected().cloned().unwrap();
    let changes = current(ChangeSummary::new(CommitId::from(BASE), files));
    ui.view().feed().unwrap().take_observation(Observation::WorkspaceChanges {
        workspace_id,
        changes,
        rev: Rev::new(3),
    });
    ui.wait("unreadable.txt read again with the next summary", |_| reads_of("unreadable.txt") == 2);
    see(&mut ui, &diff, "unreadable.txt", Shows::Says(COULDNT));
    let (pane, client) = (ui.pane().unwrap(), ui.view().client().unwrap());
    ui.cx.update_entity(&pane, |p, cx| {
        p.reconnect(&client, cx);
        p.catch_up(cx);
    });
    see(&mut ui, &diff, "unreadable.txt", Shows::Adds("read at last"));
    assert_eq!(reads_of("unreadable.txt"), 3);
}
