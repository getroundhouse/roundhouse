//! Accounts in Settings, against a real hub: connecting a login, renewing it, disconnecting it, and what the hub
//! refuses. Every token is plainly made up.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::model::{Account, AccountId, AgentKind, LoginToken};

use super::{open_settings, settings_showing};
use crate::support::{Stack, Ui, click, click_menu_item};

const TOKEN: &str = "a-made-up-token";
const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// Opens Accounts in Settings, once the hub has said what makes a login, so Connect account is on.
fn in_accounts<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    to_accounts(&mut ui);
    ui
}

fn to_accounts(ui: &mut Ui<'_>) {
    open_settings(ui);
    ui.click("settings-page-accounts");
    ui.wait("Connect account to be on", |ui| ui.has("accounts-connect") && !ui.disabled("accounts-connect"));
}

/// The hub's record of the account `id`, as the app shows it.
fn record(ui: &Ui<'_>, id: &AccountId) -> Option<Account> {
    ui.view().records().account(id).cloned()
}

/// The account the person called `name`, as the app shows it.
fn called(ui: &Ui<'_>, name: &str) -> Option<Account> {
    ui.view().records().accounts().find(|account| account.name.as_deref() == Some(name)).cloned()
}

/// Every account the hub has, as it says when asked, rather than as the app shows them.
fn the_hubs(stack: &Stack) -> Vec<Account> {
    stack.client().records_list().unwrap().accounts
}

/// A login the hub was given from elsewhere, called `name`.
fn connected(stack: &Stack, name: &str, token: &str) -> AccountId {
    stack.client().account_connect(AgentKind::Claude, name, LoginToken::new(token), None).unwrap().id
}

fn id(prefix: &str, account: &AccountId) -> SharedString {
    format!("{prefix}-{account}").into()
}

/// What the line under an account's name says, as it tells assistive apps.
fn stands(ui: &mut Ui<'_>, account: &AccountId) -> String {
    ui.frame();
    let line = id("account-stands", account);
    ui.cx.update_window(ui.handle, |_, window, _| window.find(line).label().unwrap().to_string()).unwrap()
}

/// What's in the field that has the cursor.
fn typed_where_the_cursor_is(ui: &mut Ui<'_>) -> Option<String> {
    ui.frame();
    ui.cx
        .update_window(ui.handle, |_, window, cx| window.focused_input(cx).map(|input| input.value(cx).to_string()))
        .unwrap()
}

/// Every file under `dir` that holds `text`, but for what's under a directory named `except`.
fn files_holding(dir: &Path, text: &str, except: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != except {
                found.extend(files_holding(&path, text, except));
            }
        } else if std::fs::read(&path).is_ok_and(|bytes| bytes.windows(text.len()).any(|at| at == text.as_bytes())) {
            found.push(path);
        }
    }
    found
}

#[gpui_kit::test]
fn choosing_a_default_updates_the_hub_and_other_clients(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let personal = connected(&stack, "Personal", "another-made-up-token");
    let mut ui = in_accounts(cx, &stack);
    let selected =
        |ui: &Ui<'_>| ui.view().records().hub_settings().unwrap().account_defaults.as_ref().unwrap()[0].account.clone();
    assert_eq!(selected(&ui), work);
    ui.click("account-default-claude");
    ui.click_menu_item("Personal");
    ui.wait("the new default", |ui| selected(ui) == personal);
    let hub = stack.client().records_list().unwrap().hub_settings.unwrap();
    assert_eq!(hub.account_defaults.unwrap()[0].account, personal);
}

#[gpui_kit::test]
fn a_login_connected_here_is_listed_under_added_and_its_token_is_the_hubs_alone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_accounts(cx, &stack);
    assert!(ui.has("accounts-none-added"));

    ui.click("accounts-connect");
    ui.input(TOKEN);
    assert!(ui.disabled("accounts-panel-connect"), "it needs a name too");
    ui.click("accounts-name");
    ui.input("Build machines");
    ui.click("accounts-panel-connect");

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
}

#[gpui_kit::test]
fn renewing_puts_a_new_token_in_place_of_the_accounts_own(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = in_accounts(cx, &stack);

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
}

#[gpui_kit::test]
fn disconnecting_asks_first_and_leaves_the_account_to_connect_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = in_accounts(cx, &stack);

    ui.click(id("account-more", &account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Cancel");
    ui.wait_for_every_call();
    assert!(the_hubs(&stack)[0].held_login.is_some(), "nothing is forgotten without asking");

    ui.click(id("account-more", &account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Disconnect");

    ui.wait("Connect again", |ui| ui.has(id("account-again", &account)));
    assert_eq!(record(&ui, &account).unwrap().held_login, None);
    assert_eq!(the_hubs(&stack)[0].held_login, None);
    assert!(!ui.has(id("account-more", &account)), "with nothing to renew or disconnect");
}

#[gpui_kit::test]
fn a_token_the_hub_refuses_says_why_and_is_gone_from_its_field(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_accounts(cx, &stack);
    ui.click("accounts-connect");
    ui.input("a made-up token");
    ui.click("accounts-name");
    ui.input("Work");

    ui.click("accounts-panel-connect");

    ui.wait("the refusal", |ui| ui.has("accounts-refused"));
    assert!(ui.has("accounts-panel"), "it stays open, to paste the token again");
    assert_eq!(typed_where_the_cursor_is(&mut ui).as_deref(), Some(""), "the token's field has the cursor, empty");
    assert!(ui.disabled("accounts-panel-connect"));
    assert!(ui.view().records().accounts().next().is_none());
}

#[gpui_kit::test]
fn a_token_connected_already_is_refused_for_a_new_account_and_the_one_it_has_keeps_its_name(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let mut ui = in_accounts(cx, &stack);
    ui.click("accounts-connect");
    ui.input(TOKEN);
    ui.click("accounts-name");
    ui.input("Home");

    ui.click("accounts-panel-connect");

    ui.wait("the refusal", |ui| ui.has("accounts-refused"));
    assert_eq!(refused(&mut ui), "This token is connected already, under another name.");
    let accounts = the_hubs(&stack);
    assert_eq!(accounts.iter().map(|account| &account.id).collect::<Vec<_>>(), [&work]);
    assert_eq!(accounts[0].name.as_deref(), Some("Work"), "it isn't renamed");
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
    let mut ui = in_accounts(cx, &stack);

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

#[gpui_kit::test]
fn once_the_panel_goes_the_keys_still_reach_settings(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = in_accounts(cx, &stack);
    ui.click(id("account-more", &account));
    ui.click_menu_item("Renew");
    ui.input("another-made-up-token");
    ui.press("enter");
    ui.wait("the panel to go", |ui| !ui.has("accounts-panel"));

    ui.press("escape");

    assert!(!settings_showing(&mut ui), "Escape goes back");
}

/// Held still a minute before a login starts to warn of its end, then a minute after it, with nothing changed that
/// would draw the page again, so only its own clock can.
#[gpui_kit::test]
fn a_login_starts_to_warn_a_month_before_it_ends_with_nothing_else_drawing_the_page_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let account = connected(&stack, "Work", TOKEN);
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the account", |ui| record(ui, &account).is_some());
    let warns_at_ms = record(&ui, &account).unwrap().held_login.unwrap().ends_at_ms.unwrap() - 30 * DAY_MS;
    ui.cx.update(|cx| look::hold_still(warns_at_ms - MINUTE_MS, cx));
    to_accounts(&mut ui);
    assert!(!stands(&mut ui, &account).contains("Ends in"), "{}", stands(&mut ui, &account));

    ui.cx.update(|cx| look::hold_still(warns_at_ms + MINUTE_MS, cx));
    ui.cx.executor().advance_clock(Duration::from_mins(2));
    ui.frame();

    assert!(stands(&mut ui, &account).contains("Ends in"), "{}", stands(&mut ui, &account));
}

/// More logins than fit, each the hub was given from elsewhere, in the order Added lists them.
fn more_than_fit(stack: &Stack) -> Vec<AccountId> {
    (1..=16).map(|at| connected(stack, &format!("Account {at:02}"), &format!("{TOKEN}-{at}"))).collect()
}

#[gpui_kit::test]
fn renewing_a_login_opens_its_panel_at_its_row_in_sight(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let accounts = more_than_fit(&stack);
    let mut ui = in_accounts(cx, &stack);
    let last = accounts.last().unwrap();
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(ui.visible(id("account-more", last)), "the last row is at the bottom");
    ui.click(id("account-more", last));

    // The scroll is set as the panel is first laid out, during a draw, so it shows only in a frame drawn after. The
    // page asks for one, since nothing else may draw again. Here the panel's first draw happens to bring another, so
    // that the page asked is checked too.
    ui.cx.update_window(ui.handle, |_, window, cx| click_menu_item(window, "Renew", cx)).unwrap();
    assert_eq!(ui.bring_the_next_frame(), 1, "the page asks for the frame that shows the scroll");

    let shows = ui.cx.update_window(ui.handle, |_, window, _| window.find("accounts-panel-connect").visible());
    assert!(shows.unwrap(), "the whole panel shows");
    let (row, panel) = (ui.bounds(id("account", last)), ui.bounds("accounts-panel"));
    assert!(row.top() < panel.top() && panel.bottom() <= row.bottom(), "under its row: {row:?}, {panel:?}");
}

/// As a Mac's own pages do, which a test's window holds still unless it's let move.
#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_page_pulled_past_its_top_stretches_as_a_macs_does(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, ScrollEvent, TouchPhase};

    let stack = Stack::new();
    let first = more_than_fit(&stack).remove(0);
    let mut ui = in_accounts(cx, &stack);
    let rests_at = ui.bounds(id("account", &first)).top();
    ui.allow_motion();

    let on = ui.bounds("settings-body").center();
    for (phase, by) in [(TouchPhase::Started, 0.), (TouchPhase::Moved, 100.), (TouchPhase::Moved, 100.)] {
        let pulled = ScrollEvent {
            position: on,
            delta: ScrollDelta::Pixels(point(px(0.), px(by))),
            touch_phase: phase,
            ..Default::default()
        };
        ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(pulled.to_platform_input(), cx)).unwrap();
        ui.frame();
    }
    ui.bring_the_next_frame();
    ui.frame();

    let pulled_to = ui.bounds(id("account", &first)).top();
    assert!(pulled_to > rests_at, "it stretches down from {rests_at:?}, to {pulled_to:?}");
}
