//! Agents in Settings, against a real hub: connecting a login, renewing it, disconnecting it, choosing the default,
//! reserving part of an account's week, and what the hub refuses. What the page draws of logins a hub holds is in
//! `agents_page.rs`, on a fake hub. Every token is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::{Account, AccountId, AgentKind, LoginToken, Reserve};

use super::{open_settings, settings_showing};
use crate::support::{Stack, Ui, click, files_holding};

const TOKEN: &str = "a-made-up-token";

/// Opens Agents in Settings, once the hub has said what makes a login, so Connect account is on.
pub(super) fn in_agents<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    to_agents(&mut ui);
    ui
}

pub(super) fn to_agents(ui: &mut Ui<'_>) {
    open_settings(ui);
    ui.click("settings-page-agents");
    ui.wait("Connect account to be on", |ui| ui.on("accounts-connect"));
}

/// The hub's record of the account `id`, as the app shows it.
pub(super) fn record(ui: &Ui<'_>, id: &AccountId) -> Option<Account> {
    ui.view().records().account(id).cloned()
}

/// The account the person called `name`, as the app shows it.
fn called(ui: &Ui<'_>, name: &str) -> Option<Account> {
    ui.view().records().accounts().find(|account| account.name.as_deref() == Some(name)).cloned()
}

/// Every account the hub has, as it says when asked, rather than as the app shows them.
pub(super) fn the_hubs(stack: &Stack) -> Vec<Account> {
    stack.client().records_list().unwrap().accounts
}

/// A login the hub was given from elsewhere, called `name`.
pub(super) fn connected(stack: &Stack, name: &str, token: &str) -> AccountId {
    stack.client().account_connect(AgentKind::Claude, name, LoginToken::new(token), None).unwrap().id
}

pub(super) fn id(prefix: &str, account: &AccountId) -> SharedString {
    format!("{prefix}-{account}").into()
}

/// What an account's row says after how its login was made, as it tells assistive apps.
pub(super) fn says(ui: &mut Ui<'_>, account: &AccountId) -> String {
    ui.frame();
    let said = format!("account-{account}-says");
    ui.cx.update_window(ui.handle, |_, window, _| window.find(said).label().unwrap().to_string()).unwrap()
}

/// What's in the field that has the cursor.
fn typed_where_the_cursor_is(ui: &mut Ui<'_>) -> Option<String> {
    ui.frame();
    ui.cx
        .update_window(ui.handle, |_, window, cx| window.focused_input(cx).map(|input| input.value(cx).to_string()))
        .unwrap()
}

#[gpui_kit::test]
fn choosing_a_default_updates_the_hub_and_other_clients_and_the_badge_moves_with_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let personal = connected(&stack, "Personal", "another-made-up-token");
    let mut ui = in_agents(cx, &stack);
    let selected =
        |ui: &Ui<'_>| ui.view().records().hub_settings().unwrap().account_defaults.as_ref().unwrap()[0].account.clone();
    let badge = |account| format!("{}-default", id("account", account));
    assert_eq!(selected(&ui), work);
    assert!(ui.has(badge(&work)), "the first connected is the default, and alone has the badge");
    assert!(!ui.has(badge(&personal)));

    ui.click("account-default-claude");
    ui.click_menu_item("Personal");

    ui.wait("the new default", |ui| selected(ui) == personal);
    let hub = stack.client().records_list().unwrap().hub_settings.unwrap();
    assert_eq!(hub.account_defaults.unwrap()[0].account, personal);
    ui.wait("the badge to move", |ui| ui.has(badge(&personal)) && !ui.has(badge(&work)));
}

#[gpui_kit::test]
fn choosing_automatic_takes_the_badge_away_until_an_account_is_chosen_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let mut ui = in_agents(cx, &stack);
    let automatic =
        |ui: &Ui<'_>| ui.view().records().hub_settings().unwrap().account_defaults.as_ref().unwrap()[0].automatic;
    let badge = format!("{}-default", id("account", &work));

    ui.click("account-default-claude");
    ui.click_menu_item("Automatic, Most to spare");

    ui.wait("Automatic", |ui| automatic(ui));
    assert!(stack.client().records_list().unwrap().hub_settings.unwrap().account_defaults.unwrap()[0].automatic);
    ui.wait("the badge to go", |ui| !ui.has(badge.clone()));

    ui.click("account-default-claude");
    ui.click_menu_item("Work");

    ui.wait("Work again", |ui| !automatic(ui));
    ui.wait("the badge to come back", |ui| ui.has(badge.clone()));
}

#[gpui_kit::test]
fn reserving_part_of_a_week_waits_for_save_then_the_hub_and_the_app_have_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let mut ui = in_agents(cx, &stack);

    ui.click(id("account-more", &work));
    ui.click_menu_item("Reserve usage…");
    ui.wait("the panel", |ui| ui.has("reserve-panel"));
    ui.click("reserve-25");
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack)[0].reserve, None, "nothing is saved until Save");
    ui.click("reserve-save");

    ui.wait("the reserve", |ui| record(ui, &work).is_some_and(|account| account.reserve == Reserve::new(25)));
    assert_eq!(the_hubs(&stack)[0].reserve, Reserve::new(25));
    ui.wait("the panel to go", |ui| !ui.has("reserve-panel"));
    ui.click(id("account-more", &work));
    ui.click_menu_item("Reserve usage…");
    ui.wait("the panel again", |ui| ui.has("reserve-panel"));
    ui.click("reserve-0");
    ui.click("reserve-save");
    ui.wait("none reserved", |ui| record(ui, &work).is_some_and(|account| account.reserve.is_none()));
}

/// One walk through connecting a login: first what the hub refuses as no token, then the token in the same panel,
/// then the same token again for another account, which the hub refuses as connected already.
#[gpui_kit::test]
fn a_login_connected_here_is_listed_under_added_its_token_the_hubs_alone_and_a_refused_one_says_why(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_agents(cx, &stack);
    assert!(ui.has("accounts-none-added"));

    ui.click("accounts-connect");
    ui.input("a made-up token");
    assert!(ui.off("accounts-panel-connect"), "it needs a name too");
    ui.click("accounts-name");
    ui.input("Build machines");
    ui.click("accounts-panel-connect");
    ui.wait("the refusal of what isn't a token", |ui| ui.has("accounts-refused"));
    assert!(ui.has("accounts-panel"), "what isn't a token: it stays open, to paste the token again");
    assert_eq!(
        typed_where_the_cursor_is(&mut ui).as_deref(),
        Some(""),
        "what isn't a token: the token's field has the cursor, empty"
    );
    assert!(ui.off("accounts-panel-connect"), "what isn't a token: Connect is off with the field empty");
    assert!(ui.view().records().accounts().next().is_none(), "what isn't a token: nothing was connected");

    // By Enter, since the panel, grown by what the hub said, reaches below the window here, and the page doesn't
    // scroll it into sight again.
    ui.input(TOKEN);
    ui.press("enter");

    ui.wait("the account", |ui| called(ui, "Build machines").is_some() && !ui.has("accounts-panel"));
    let account = called(&ui, "Build machines").unwrap();
    assert_eq!(account.login_agent, Some(AgentKind::Claude));
    assert!(account.held_login.is_some());
    assert!(ui.has(id("account-more", &account.id)), "it's listed, with its menu");
    assert!(!ui.has("accounts-none-added"));
    let everything = stack.dir.path();
    assert_eq!(
        files_holding(everything, TOKEN, "logins"),
        Vec::<PathBuf>::new(),
        "nothing but the hub's logins has it"
    );
    assert_eq!(files_holding(everything, TOKEN, "").len(), 1, "which do");

    ui.click("accounts-connect");
    ui.input(TOKEN);
    ui.click("accounts-name");
    ui.input("Home");
    ui.click("accounts-panel-connect");

    ui.wait("the refusal of a token connected already", |ui| ui.has("accounts-refused"));
    assert_eq!(
        refused(&mut ui),
        "This token is connected already, under another name.",
        "connected already: the panel says so"
    );
    let accounts = the_hubs(&stack);
    assert_eq!(accounts.iter().map(|account| &account.id).collect::<Vec<_>>(), [&account.id]);
    assert_eq!(accounts[0].name.as_deref(), Some("Build machines"), "connected already: it isn't renamed");
}

#[gpui_kit::test]
fn renewing_puts_a_new_token_in_place_of_the_accounts_own_and_then_the_keys_still_reach_settings(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = in_agents(cx, &stack);

    ui.click(id("account-more", &account));
    ui.click_menu_item("Renew");
    assert!(!ui.has("accounts-name"), "the account keeps its name");
    ui.input("another-made-up-token");
    ui.press("enter");

    ui.wait("the new login", |ui| !ui.has("accounts-panel"));
    let logins = stack.hub.paths.data_dir.join("logins");
    assert_eq!(files_holding(&logins, "another-made-up-token", "").len(), 1);
    assert!(files_holding(&logins, TOKEN, "").is_empty(), "the old one is forgotten");
    let accounts = the_hubs(&stack);
    assert_eq!(accounts.iter().map(|account| &account.id).collect::<Vec<_>>(), [&account], "it's the same account");
    assert_eq!(accounts[0].name.as_deref(), Some("Work"));

    ui.press("escape");

    assert!(!settings_showing(&mut ui), "once the panel goes, Escape goes back");
}

#[gpui_kit::test]
fn disconnecting_asks_first_and_leaves_the_account_to_connect_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = in_agents(cx, &stack);

    ui.click(id("account-more", &account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("disconnect-agent-account-keep");
    ui.wait_for_every_call();
    assert!(the_hubs(&stack)[0].held_login.is_some(), "nothing is forgotten without asking");

    ui.click(id("account-more", &account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("disconnect-agent-account-act");

    ui.wait("it disconnected", |ui| record(ui, &account).is_some_and(|account| account.held_login.is_none()));
    assert_eq!(the_hubs(&stack)[0].held_login, None);
    assert_eq!(says(&mut ui, &account), "Disconnected");
    ui.click(id("account-more", &account));
    assert!(ui.labelled("Disconnect").is_empty(), "with nothing to disconnect");
    ui.click_menu_item("Connect again");
    assert!(!ui.has("accounts-name"), "the account keeps its name");
    ui.input("another-made-up-token");
    ui.press("enter");

    ui.wait("the login again", |ui| record(ui, &account).is_some_and(|account| account.held_login.is_some()));
    let accounts = the_hubs(&stack);
    assert_eq!(accounts.iter().map(|account| &account.id).collect::<Vec<_>>(), [&account], "it's the same account");
    assert!(accounts[0].held_login.is_some());
}

/// What the panel says the hub refused, as it tells assistive apps.
fn refused(ui: &mut Ui<'_>) -> String {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find("accounts-refused").label().unwrap().to_string()).unwrap()
}

/// A first token the hub refuses, for a panel cancelled before the hub answers, then a second in another panel, all
/// before any answer is taken, so the refusal comes while the second waits.
#[gpui_kit::test]
fn what_the_hub_answers_a_panel_that_went_lands_on_no_other(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_agents(cx, &stack);

    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            let connect = |window: &mut _, token: &str, name: &str, cx: &mut _| {
                click(window, "accounts-connect", cx);
                window.input(token, cx);
                click(window, "accounts-name", cx);
                window.input(name, cx);
                click(window, "accounts-panel-connect", cx);
            };
            connect(window, "a made-up token", "First", cx);
            click(window, "accounts-cancel", cx);
            connect(window, TOKEN, "Second", cx);
        })
        .unwrap();

    ui.wait_for_every_call();
    ui.wait("the second", |ui| called(ui, "Second").is_some());
    assert!(!ui.has("accounts-panel"), "the second's panel went as it connected");
    assert!(!ui.has("accounts-refused"));
    assert_eq!(the_hubs(&stack).len(), 1, "and the first was refused");
}
