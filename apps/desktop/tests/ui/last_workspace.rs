//! The window opening on the workspace it was left on. Which that is, and what's saved of it, is the app's own, so these
//! open on a fake hub. `client_core::this_device`'s unit tests have what's kept of it.

#![cfg(test)]

use client_core::this_device::Preferences;
use gpui_kit::{App, TestAppContext};
use protocol::model::WorkspaceStatus;

use crate::support::{FakeHub, TIMEOUT, Ui, project_row, sidebar_row, two_projects_world};

/// Each test app has its own memory, so the second has only what the first saved.
#[gpui_kit::test]
fn the_window_opens_on_the_workspace_it_was_left_on_in_a_project_still_collapsed(
    first: &mut TestAppContext,
    again: &mut TestAppContext,
) {
    let (world, [project, _], [upper, lower, third, _]) = two_projects_world();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(first, &fake);
    ui.wait("the top of the sidebar", |ui| ui.view().selected() == Some(&upper));
    for workspace in [&third, &lower] {
        ui.click(sidebar_row(workspace));
        ui.wait("the workspace", |ui| ui.view().selected() == Some(workspace));
    }
    ui.click(project_row(&project));
    ui.cx.update(App::shutdown);

    let mut ui = Ui::open_on(again, &fake);

    ui.wait("the workspace's chat", |ui| ui.transcript().is_some());
    assert_eq!(ui.view().selected(), Some(&lower));
    assert!(!ui.has(sidebar_row(&lower)) && ui.has(sidebar_row(&third)), "its project is still collapsed");
    assert!(!ui.cx.update(|cx| cx.is_action_available(&desktop::GoBack)), "it's where the window starts");
}

#[gpui_kit::test]
fn a_window_left_on_a_workspace_archived_since_opens_on_the_top_of_the_sidebar(cx: &mut TestAppContext) {
    let (mut world, _, [top, archived, ..]) = two_projects_world();
    world.workspaces.iter_mut().find(|w| w.id == archived).unwrap().status = WorkspaceStatus::Archived;
    let fake = FakeHub::holding(world);
    let mut preferences = Preferences::in_dir(&fake.source().paths.data_dir, |failure| panic!("{failure}"));
    preferences.left_on(None, Some(&archived));
    preferences.saved().recv_timeout(TIMEOUT).unwrap();

    let mut ui = Ui::open_on(cx, &fake);

    ui.wait("a workspace's chat", |ui| ui.transcript().is_some());
    assert_eq!(ui.view().selected(), Some(&top));
    assert_eq!(ui.view().error, None);
}
