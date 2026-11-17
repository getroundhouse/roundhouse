//! The `…` menu of each row on Settings' account pages, Agents and Git, which opens inside the page's column, on a
//! fake hub, since opening one asks the hub for nothing. The default's badge moving with the hub's choice is in
//! `agents.rs`, where the default is chosen. Every account is plainly made up.

#![cfg(test)]

use gpui_kit::{Bounds, Pixels, SharedString, TestAppContext};

use super::agents::id;
use super::git::more;
use super::made_up::{DAY_MS, added, holding_logins, in_agents_of, with_ada};
use crate::support::{MENU, Ui};

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

/// One walk through one window, a case for each kind of account: an agent's in Agents, then a GitHub account's in Git.
#[gpui_kit::test]
fn an_accounts_menu_opens_inside_the_pages_column_whatever_kind_of_account_it_is(cx: &mut TestAppContext) {
    let work = added("claude-login-0001", "Work", 200 * DAY_MS);
    let mut world = holding_logins(vec![work.clone()]);
    let ada = with_ada(&mut world);
    let (_hub, mut ui) = in_agents_of(cx, world);

    let (menu, row) = menu_and_row(&mut ui, id("account-more", &work.id), id("account", &work.id));
    assert!(inside(menu, row), "an agent account's menu, {menu:?}, reaches past the row, {row:?}");

    ui.press("escape");
    ui.wait("the agent account's menu to close", |ui| !ui.has(MENU));
    ui.click("settings-page-git");
    ui.wait("the GitHub account's row", |ui| ui.has(more(&ada.id)));
    let (menu, row) = menu_and_row(&mut ui, more(&ada.id), format!("git-account-{}", ada.id).into());

    assert!(inside(menu, row), "a GitHub account's menu, {menu:?}, reaches past the row, {row:?}");
}
