//! The sidebar's list of workspaces.

#![cfg(test)]

use gpui_kit::{ElementId, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::model::WorkspaceId;

use crate::support::{Stack, Ui};

/// More than the sidebar has room for in the window.
const WORKSPACES: usize = 40;

fn row(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("workspace-{id}")))
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
