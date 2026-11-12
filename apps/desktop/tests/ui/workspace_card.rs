//! The card a workspace's name in its header shows while the pointer rests on it: where its worktree is, with a button
//! that copies the path, which it waits for the pointer to cross to.

#![cfg(test)]

use gpui_kit::TestAppContext;

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn resting_on_a_workspace_s_name_shows_where_it_is_and_its_button_copies_the_path_until_the_pointer_has_gone(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let id = ui.new_workspace();
    let path = |ui: &Ui| ui.view().records().workspace(&id).and_then(|workspace| workspace.path.clone());
    ui.wait("the worktree to be made", |ui| path(ui).is_some());

    ui.hover("workspace-title");
    assert!(!ui.has("workspace-card"), "passing over the name shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("workspace-card"), "resting on it does");
    ui.click("copy-workspace-path");

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, path(&ui), "the worktree's whole path");

    // The composer's button, well away from the card, which covers what's under the header.
    ui.hover("send");
    assert!(ui.visible("workspace-card"), "the pointer could still be on its way to the button");
    ui.wait_out_a_tooltip();
    ui.wait("the card to go", |ui| !ui.has("workspace-card"));
}
