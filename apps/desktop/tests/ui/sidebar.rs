//! The sidebar's list of workspaces.

#![cfg(test)]

use std::path::PathBuf;

use desktop::workspace_pane::Busy;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{Stack, Ui, commit_settings};

/// More than the sidebar has room for in the window.
const WORKSPACES: usize = 40;

fn row(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("workspace-{id}")))
}

fn archive_button(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("archive-{id}")))
}

fn archived(ui: &Ui, id: &WorkspaceId) -> bool {
    let listed = ui.view().workspaces.iter().any(|w| &w.id == id && w.status == WorkspaceStatus::Archived);
    listed && ui.view().busy == Busy::Free
}

/// An archive script that holds archiving open until the test lets it go, and counts how often it has run.
struct HeldArchive {
    released: PathBuf,
    runs: PathBuf,
}

impl HeldArchive {
    /// Commits the script, which every workspace made from now on runs.
    fn commit(stack: &Stack) -> Self {
        let (released, runs) = (stack.repo().with_file_name("released"), stack.repo().with_file_name("runs"));
        let script =
            format!("echo ran >> {}; until [ -f {} ]; do sleep 0.01; done", runs.display(), released.display());
        commit_settings(stack, &format!("[scripts]\narchive = \"{script}\"\n"));
        Self { released, runs }
    }

    fn release(&self) {
        std::fs::write(&self.released, "").unwrap();
    }

    fn runs(&self) -> usize {
        std::fs::read_to_string(&self.runs).map_or(0, |runs| runs.lines().count())
    }
}

/// Waits until no call the app started is under way, then for the app to take their replies, so whatever a call
/// pressed by mistake would have done has been done.
fn wait_for_every_call(ui: &mut Ui) {
    let finished = ui.view().client().unwrap().commands().finished();
    ui.wait("every call to end", |_| finished.try_recv().is_ok());
    ui.frame();
}

/// Whether the last frame drew the element with ID `id` as a button that can't be pressed.
fn disabled(ui: &mut Ui, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    ui.frame();
    let found = ui.cx.update_window(ui.handle, |_, window, _| window.try_find(id).and_then(|e| e.disabled())).unwrap();
    found == Some(true)
}

fn open_with_a_repository<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui
}

#[gpui_kit::test]
fn a_long_list_draws_only_the_workspaces_on_screen_and_scrolls_to_the_rest(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    for _ in 0..WORKSPACES {
        client.workspace_create(&repo.id, None, None, None).unwrap();
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == WORKSPACES);
    let listed: Vec<WorkspaceId> = ui.view().visible_workspaces().iter().map(|w| w.id.clone()).collect();
    let (first, last) = (row(&listed[0]), row(&listed[WORKSPACES - 1]));

    assert!(ui.has(first.clone()));
    assert!(!ui.has(last.clone()), "a row below the bottom isn't drawn");

    // Over a row, since the kit's test window only finds what it records, and the list itself isn't.
    ui.scroll(first.clone(), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));

    assert!(ui.has(last), "scrolling reaches the last row");
    assert!(!ui.has(first), "a row above the top isn't drawn");
}

#[gpui_kit::test]
fn a_rows_archive_button_shows_under_the_pointer_and_archives_it_without_opening_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let held = HeldArchive::commit(&stack);
    let mut ui = open_with_a_repository(cx, &stack);
    let (going, showing) = (ui.new_workspace(), ui.new_workspace());
    ui.hover("send");
    assert!(!ui.visible(archive_button(&going)), "the pointer is elsewhere");
    assert!(!ui.visible(archive_button(&showing)), "not even the workspace showing has one");
    ui.hover(row(&going));
    assert!(ui.visible(archive_button(&going)), "the pointer is over its row");
    assert!(!ui.visible(archive_button(&showing)), "only the row under the pointer has one");

    ui.click(archive_button(&going));

    ui.wait("it to start archiving", |_| held.runs() == 1);
    // Held open, so nothing that comes once it's archived has moved the selection yet.
    assert_eq!(ui.view().selected.as_ref(), Some(&showing), "pressing it doesn't also open the workspace");
    held.release();
    ui.wait("it to be archived", |ui| archived(ui, &going));
    assert!(!ui.has(row(&going)), "it leaves the list");
}

#[gpui_kit::test]
fn a_rows_menu_archives_it_and_an_archived_ones_doesnt_offer_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();

    ui.right_click(row(&id));
    ui.click_menu_item("Archive");

    ui.wait("it to be archived", |ui| archived(ui, &id));
    ui.click("toggle-archived");
    ui.right_click(row(&id));
    assert!(ui.shows_label("popup-menu", "Rename…"), "its menu is open");
    assert!(!ui.shows_label("popup-menu", "Archive"));
    assert!(!ui.has(archive_button(&id)), "pressing the row restores it, so it has no button to archive it either");
}

#[gpui_kit::test]
fn nothing_else_is_archived_or_restored_while_a_workspace_is_being_archived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repository(cx, &stack);
    // Made before the script, so archiving it isn't held.
    let earlier = ui.new_workspace();
    stack.client().workspace_archive(&earlier, false).unwrap();
    ui.wait("the earlier one to be archived", |ui| archived(ui, &earlier));
    let held = HeldArchive::commit(&stack);
    ui.click("toggle-archived");
    let id = ui.new_workspace();

    ui.press("cmd-shift-a");
    ui.wait("it to start archiving", |_| held.runs() == 1);
    assert!(disabled(&mut ui, "archive"), "the header's button can't be pressed meanwhile");
    ui.hover(row(&id));
    assert!(disabled(&mut ui, archive_button(&id)), "nor can the row's");
    ui.press("cmd-shift-a");
    ui.click(row(&earlier));
    held.release();

    wait_for_every_call(&mut ui);
    assert_eq!(held.runs(), 1, "the second press didn't archive it again");
    assert_eq!(ui.view().error, None);
    assert!(archived(&ui, &id));
    let workspaces = stack.client().workspace_list(true).unwrap();
    let restored = workspaces.iter().any(|w| w.id == earlier && w.status != WorkspaceStatus::Archived);
    assert!(!restored, "pressing an archived row meanwhile didn't restore it");
}
