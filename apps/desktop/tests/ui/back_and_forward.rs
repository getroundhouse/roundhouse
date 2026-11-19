//! Back and Forward, through the workspaces the window has shown. What they come to is `app::selection`'s rule, which
//! its unit tests pin, so this shows that the keys reach it, from the sidebar and from Home, and that the menus grey out
//! the way there's nowhere to go, past a workspace archived since too. Archiving is the hub's, so this opens on a real
//! one.

#![cfg(test)]

use gpui_kit::{Action, TestAppContext};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{Stack, Ui, sidebar_row};

/// Whether the menus offer `action`, which is whether anything where focus is takes it.
fn offered(ui: &mut Ui<'_>, action: &dyn Action) -> bool {
    ui.frame();
    ui.cx.update(|cx| cx.is_action_available(action))
}

/// Presses `key` and waits until `workspace` shows, with Home put away.
fn press_to(ui: &mut Ui<'_>, key: &str, workspace: &WorkspaceId) {
    ui.press(key);
    ui.wait(key, |ui| ui.view().selected() == Some(workspace) && !ui.has("home-page"));
}

fn show_home(ui: &mut Ui<'_>) {
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
}

#[gpui_kit::test]
fn back_and_forward_go_through_the_workspaces_shown_in_the_order_they_were(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let [first, second, third] = [(); 3].map(|()| ui.new_workspace());
    assert!(!offered(&mut ui, &desktop::GoForward), "nowhere to go forward to yet");

    press_to(&mut ui, "secondary-[", &second);
    press_to(&mut ui, "secondary-[", &first);
    assert!(!offered(&mut ui, &desktop::GoBack), "the first is where it started");
    press_to(&mut ui, "secondary-]", &second);
    show_home(&mut ui);
    press_to(&mut ui, "secondary-[", &second);
    assert!(offered(&mut ui, &desktop::GoForward), "Back from Home comes back to what it covered, and no further");
    show_home(&mut ui);
    press_to(&mut ui, "secondary-]", &third);
    press_to(&mut ui, "secondary-[", &second);

    ui.click(sidebar_row(&first));
    ui.wait("the first", |ui| ui.view().selected() == Some(&first));
    assert!(!offered(&mut ui, &desktop::GoForward), "choosing one forgets what Back came back from");
    assert!(offered(&mut ui, &desktop::GoBack), "and Back comes back to the second");

    stack.client().workspace_archive(&second, false).unwrap();
    ui.wait("the second to be archived", |ui| {
        ui.view().records().workspaces().iter().any(|w| w.id == second && w.status == WorkspaceStatus::Archived)
    });
    assert!(!offered(&mut ui, &desktop::GoBack), "past the second, archived, is the first, which shows");
}
