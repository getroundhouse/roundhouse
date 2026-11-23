//! Home, which lists every workspace in place of the one showing. What it shows of the workspaces the hub holds, and
//! where the keys go, is the app's, so those open on a fake hub, with no worker to wait for. Archiving and restoring
//! need the real one.

#![cfg(test)]

use client_core::fake_hub::World;
use gpui_kit::{SharedString, TestAppContext};
use protocol::model::{LineCounts, Observation, Observed, Rev, WorkspaceId, WorkspaceStatus};

use crate::support::{FakeHub, Held, Stack, Ui, home_row, open_on_the_world, with_a_chat};

fn has(ui: &Ui, id: &WorkspaceId, status: WorkspaceStatus) -> bool {
    ui.view().records().workspaces().iter().any(|w| &w.id == id && w.status == status)
}

/// Opens Home over the workspace showing.
fn to_home(ui: &mut Ui) {
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
}

/// A window with a repository and two workspaces, the second showing, and Home opened over it. Returns them.
fn open_home<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, WorkspaceId, WorkspaceId) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let first = ui.new_workspace();
    let second = ui.new_workspace();
    to_home(&mut ui);
    (ui, first, second)
}

/// [`open_home`] on a fake hub, which goes with the test, so it's returned to be kept, with the workspace that isn't
/// showing first.
fn open_home_on_a_fake_hub(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, WorkspaceId, WorkspaceId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let made = [with_a_chat(&mut world, &repo, "ledger"), with_a_chat(&mut world, &repo, "points")];
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    let showing = ui.view().selected().cloned().unwrap();
    let first = made.into_iter().find(|w| *w != showing).unwrap();
    to_home(&mut ui);
    (fake, ui, first, showing)
}

#[gpui_kit::test]
fn home_shows_in_place_of_the_workspace_until_one_is_picked(cx: &mut TestAppContext) {
    let (_hub, mut ui, first, second) = open_home_on_a_fake_hub(cx);

    assert!(!ui.has("workspace-title"), "the workspace under it is hidden");
    assert!(ui.has(home_row(&first)) && ui.has(home_row(&second)));
    ui.click(home_row(&first));

    ui.wait("the workspace", |ui| ui.view().selected() == Some(&first) && ui.has("workspace-title"));
    assert!(!ui.has("home-page"));
}

/// The sidebar's rows draw them by the same rule, which its tests go through.
#[gpui_kit::test]
fn a_row_shows_its_workspaces_lines_added_and_removed(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace_id = with_a_chat(&mut world, &repo, "ledger");
    let lines = Observed::Current { value: LineCounts { added: 12, removed: 3 } };
    world.observations =
        vec![Observation::WorkspaceLines { workspace_id: workspace_id.clone(), lines, rev: Rev::new(1) }];
    let (_fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("the workspace", |ui| ui.view().selected().is_some());

    to_home(&mut ui);

    assert!(ui.shows_label(&format!("home-lines-{workspace_id}"), "12 lines added and 3 removed"));
}

#[gpui_kit::test]
fn the_list_narrows_to_what_the_search_matches_as_its_typed(cx: &mut TestAppContext) {
    let (_hub, mut ui, first, second) = open_home_on_a_fake_hub(cx);
    let branch = ui.view().records().workspaces().iter().find(|w| w.id == first).unwrap().branch.clone();

    // Home's search has the keys, since the composer under it is hidden.
    ui.input(branch.as_str());

    ui.wait("the other workspace to go", |ui| !ui.has(home_row(&second)));
    assert!(ui.has(home_row(&first)));
}

#[gpui_kit::test]
fn the_workspaces_keys_do_nothing_while_home_hides_it(cx: &mut TestAppContext) {
    let (_hub, mut ui, _, _) = open_home_on_a_fake_hub(cx);
    assert!(ui.changes_open(), "it starts open");

    ui.press("secondary-shift-b");

    assert!(ui.changes_open(), "the sidebar of the workspace nobody can see stays as it was");
}

#[gpui_kit::test]
fn archiving_from_home_leaves_home_showing_with_the_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let held = Held::archive(&stack);
    let (mut ui, _, second) = open_home(cx, &stack);

    // The one selected, so the selection moves to another under Home once it's archived.
    ui.click(SharedString::from(format!("home-archive-{second}")));

    ui.wait("it to start archiving", |_| held.runs() == 1);
    // The row says so from the workspace's record, which reaches the app on the hub's feed, apart from the script the
    // hub runs once it has recorded it, so it's waited for rather than read as soon as the script has run.
    ui.wait("its row to say so in place of the button", |ui| {
        ui.shows_label(&format!("home-doing-{second}"), "Archiving…")
    });
    held.release();
    ui.wait("it to be archived", |ui| has(ui, &second, WorkspaceStatus::Archived) && !ui.has(home_row(&second)));
    assert!(ui.has("home-page"), "the button doesn't also show the workspace, as its row would");
    ui.input("a name no workspace has");
    ui.wait("the search to still have the keys", |ui| ui.has("home-nothing"));
}

#[gpui_kit::test]
fn an_archived_row_says_its_being_restored_until_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first, _) = open_home(cx, &stack);
    stack.client().workspace_archive(&first, false).unwrap();
    ui.wait("it to be archived", |ui| has(ui, &first, WorkspaceStatus::Archived) && ui.view().busy.is_free());
    let held = Held::checkout(&stack);
    ui.click("home-archived");
    ui.click_menu_item("Showing all");

    ui.click(SharedString::from(format!("home-restore-{first}")));

    ui.wait("it to start restoring", |_| held.runs() == 1);
    // From the workspace's record, which reaches the app on the hub's feed, apart from the checkout the hub's worker
    // makes once the hub has recorded it, so it's waited for rather than read as soon as the checkout has run.
    ui.wait("its row to say so in place of the button", |ui| {
        ui.shows_label(&format!("home-doing-{first}"), "Restoring…")
    });
    held.release();
    ui.wait("it to be restored", |ui| has(ui, &first, WorkspaceStatus::Ready) && ui.view().busy.is_free());
}

/// Archived from elsewhere while Settings shows over Home, so another workspace is selected under Home, which hides its
/// pane, and Home's search has the keys again once Settings goes.
#[gpui_kit::test]
fn going_back_from_settings_gives_homes_search_the_keys_after_the_selection_moved_under_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first, second) = open_home(cx, &stack);
    ui.press("secondary-,");
    ui.wait("Settings", |ui| ui.has("settings-back"));

    stack.client().workspace_archive(&second, false).unwrap();
    ui.wait("the other one, under Home", |ui| ui.view().selected() == Some(&first) && ui.pane().is_some());
    ui.press("escape");

    assert!(ui.has("home-page"));
    ui.input("a name no workspace has");
    ui.wait("the search to have the keys", |ui| ui.has("home-nothing"));
}
