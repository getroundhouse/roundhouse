//! The rows of Settings' account pages, Agents and Git, against a real hub: the default's badge, and each row's `…`
//! menu, which opens inside the page's column. Every token is plainly made up.

#![cfg(test)]

use gpui_kit::{Bounds, Pixels, SharedString, TestAppContext};

use super::agents::{connected, id, in_agents};
use super::git::{TOKEN, in_git, knowing_two, more};
use crate::support::{MENU, Stack, Ui};

/// Opens the menu whose button is `button`, and says where it is, and where `row` is, which is as wide as the page's
/// column.
fn menu_and_row(ui: &mut Ui<'_>, button: SharedString, row: SharedString) -> (Bounds<Pixels>, Bounds<Pixels>) {
    ui.click(button);
    ui.wait("the menu", |ui| ui.has(MENU));
    (ui.bounds(MENU), ui.bounds(row))
}

fn inside(menu: Bounds<Pixels>, row: Bounds<Pixels>) -> bool {
    row.left() <= menu.left() && menu.right() <= row.right()
}

#[gpui_kit::test]
fn the_default_agent_accounts_row_alone_has_the_badge_and_it_moves_with_the_hubs_choice(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", "a-made-up-token");
    let personal = connected(&stack, "Personal", "another-made-up-token");
    let mut ui = in_agents(cx, &stack);
    let badge = |account| format!("{}-default", id("account", account));

    assert!(ui.has(badge(&work)), "the first connected is the default");
    assert!(!ui.has(badge(&personal)));

    ui.click("account-default-claude");
    ui.click_menu_item("Personal");

    ui.wait("the badge to move", |ui| ui.has(badge(&personal)) && !ui.has(badge(&work)));
}

#[gpui_kit::test]
fn an_agent_accounts_menu_opens_inside_the_pages_column(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", "a-made-up-token");
    let mut ui = in_agents(cx, &stack);

    let (menu, row) = menu_and_row(&mut ui, id("account-more", &work), id("account", &work));

    assert!(inside(menu, row), "the menu, {menu:?}, reaches past the row, {row:?}");
}

#[gpui_kit::test]
fn a_github_accounts_menu_opens_inside_the_pages_column_as_an_agent_accounts_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let ada = super::git::connected(&stack, TOKEN);
    let mut ui = in_git(cx, &stack);
    ui.wait("its row", |ui| ui.has(more(&ada)));

    let (menu, row) = menu_and_row(&mut ui, more(&ada), format!("git-account-{ada}").into());

    assert!(inside(menu, row), "the menu, {menu:?}, reaches past the row, {row:?}");
}
