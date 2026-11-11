//! The sidebar's list of workspaces.

#![cfg(test)]

use desktop::busy::Busy;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, MouseButton, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{Held, Stack, Ui, home_row, pull_request};

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

/// Waits until no call the app started is under way, then for the app to take their replies, so whatever a call
/// pressed by mistake would have done has been done.
fn wait_for_every_call(ui: &mut Ui) {
    let finished = ui.view().client().unwrap().commands().finished();
    ui.wait("every call to end", |_| finished.try_recv().is_ok());
    ui.frame();
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
    let held = Held::archive(&stack);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
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
    ui.hover("send");
    assert!(ui.shows_label(&format!("doing-{going}"), "Archiving…"), "its row says so, wherever the pointer is");
    assert!(!ui.has(SharedString::from(format!("doing-{showing}"))), "the workspace showing isn't going");
    held.release();
    ui.wait("it to be archived", |ui| archived(ui, &going));
    assert!(!ui.has(row(&going)), "it leaves the list");
}

/// A quick press is made as the pointer comes to rest, so it moves while the button is down. Once pressed, the row says
/// the pointer has left it at the first move, which would take the button away from under the press.
#[gpui_kit::test]
fn a_rows_archive_button_stays_under_a_press_while_the_pointer_moves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let held = Held::archive(&stack);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (going, showing) = (ui.new_workspace(), ui.new_workspace());
    ui.hover(row(&going));
    let button = archive_button(&going);

    let (drawn, at) = ui
        .cx
        .update_window(ui.handle, |_, window, cx| {
            let at = window.find(button.clone()).bounds().center();
            window.pointer_move(at, None, cx);
            window.pointer_down(at, MouseButton::Left, cx);
            let at = at + point(px(1.), px(0.));
            window.pointer_move(at, Some(MouseButton::Left), cx);
            (window.try_find(button.clone()).is_some(), at)
        })
        .unwrap();

    assert!(drawn, "the button is still there while it's held");

    ui.cx.update_window(ui.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();

    ui.wait("it to start archiving", |_| held.runs() == 1);
    assert_eq!(ui.view().selected.as_ref(), Some(&showing), "letting go doesn't open the workspace instead");
    held.release();
    ui.wait("it to be archived", |ui| archived(ui, &going));
}

#[gpui_kit::test]
fn a_rows_menu_archives_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();

    ui.right_click(row(&id));
    ui.click_menu_item("Archive");

    ui.wait("it to be archived", |ui| archived(ui, &id));
    assert!(!ui.has(row(&id)), "it leaves the list, and only Home shows it");
}

#[gpui_kit::test]
fn the_heading_adds_the_first_project_and_stays_to_add_another(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.view().repos.is_empty());

    ui.click("add-project");
    let folder = stack.repo();
    ui.cx.simulate_path_prompt_response(move |_| Some(vec![folder]));

    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    assert!(ui.has("add-project"), "it's still there to add another");
}

#[gpui_kit::test]
fn nothing_else_is_archived_or_restored_while_a_workspace_is_being_archived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Merged, so the pull request's bar offers Archive too.
    let gh = stack.start_hub_with_stand_in_gh();
    gh.report(&pull_request("MERGED"));
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    // Made before the script, so archiving it isn't held.
    let earlier = ui.new_workspace();
    stack.client().workspace_archive(&earlier, false).unwrap();
    ui.wait("the earlier one to be archived", |ui| archived(ui, &earlier));
    let held = Held::archive(&stack);
    let other = ui.new_workspace();
    let id = ui.new_workspace();
    ui.wait("the pull request's Archive", |ui| ui.shows_label("pull-request-action", "Archive"));

    ui.press("cmd-shift-a");
    ui.wait("it to start archiving", |_| held.runs() == 1);
    assert!(ui.disabled("pull-request-action"), "the pull request's Archive can't be pressed meanwhile");
    assert!(ui.shows_label("pull-request-action", "Archiving…"), "and says why");
    assert!(ui.shows_label(&format!("doing-{id}"), "Archiving…"), "as its row does, in place of its button");
    ui.hover(row(&other));
    assert!(ui.disabled(archive_button(&other)), "another row's can't be pressed either");
    ui.press("cmd-shift-a");
    // Only Home lists an archived workspace, and pressing its row there brings it back.
    ui.click("home");
    ui.click("home-archived");
    ui.click_menu_item("Showing all");
    ui.click(home_row(&earlier));
    held.release();

    wait_for_every_call(&mut ui);
    assert_eq!(held.runs(), 1, "the second press didn't archive it again");
    assert_eq!(ui.view().error, None);
    assert!(archived(&ui, &id));
    let workspaces = stack.client().workspace_list(true).unwrap();
    let restored = workspaces.iter().any(|w| w.id == earlier && w.status != WorkspaceStatus::Archived);
    assert!(!restored, "pressing an archived row meanwhile didn't restore it");
}
