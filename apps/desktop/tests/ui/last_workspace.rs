//! The window opening on the workspace it was left on. Which that is, and what's saved of it, is the app's own, so these
//! open on a fake hub. `client_core::this_device`'s unit tests have what's kept of it.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::this_device::Preferences;
use gpui_kit::{App, TestAppContext};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{FakeHub, TIMEOUT, Ui, project_row, sidebar_row, two_projects_world, with_a_chat};

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

/// With none left in its project, that's the top of what the sidebar shows, not of a collapsed project above it.
#[gpui_kit::test]
fn a_window_left_on_a_workspace_archived_since_opens_on_the_first_of_its_project_or_else_the_top_of_the_sidebar(
    one_left: &mut TestAppContext,
    none_left: &mut TestAppContext,
) {
    let (mut world, _, [_, _, archived, last]) = two_projects_world();
    world.workspace_mut(&archived).status = WorkspaceStatus::Archived;
    let fake = left_on(&archived, world);

    opens_on(one_left, &fake, &last);

    let (mut world, [collapsed, _], [_, _, archived, last]) = two_projects_world();
    for gone in [&archived, &last] {
        world.workspace_mut(gone).status = WorkspaceStatus::Archived;
    }
    let third = world.repo("tracks");
    let below = with_a_chat(&mut world, &third, "below");
    let fake = left_on(&archived, world);
    std::fs::write(fake.settings_file(), format!(r#"{{"collapsed_projects": ["{collapsed}"]}}"#)).unwrap();

    opens_on(none_left, &fake, &below);
}

/// A hub holding `world`, with the app left on `workspace`.
fn left_on(workspace: &WorkspaceId, world: World) -> FakeHub {
    let fake = FakeHub::holding(world);
    let mut preferences = Preferences::in_dir(&fake.source().paths.data_dir, |failure| panic!("{failure}"));
    preferences.left_on(None, Some(workspace));
    preferences.saved().recv_timeout(TIMEOUT).unwrap();
    fake
}

fn opens_on(cx: &mut TestAppContext, fake: &FakeHub, workspace: &WorkspaceId) {
    let mut ui = Ui::open_on(cx, fake);
    ui.wait("a workspace's chat", |ui| ui.transcript().is_some());
    assert_eq!(ui.view().selected(), Some(workspace));
    assert_eq!(ui.view().error, None);
}
