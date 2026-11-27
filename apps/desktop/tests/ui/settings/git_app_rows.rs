//! An app of your own's rows in Git, against a real hub that calls a stand-in for GitHub, with the app made by asking
//! the hub as the panel would: an installation's row as the default, its menu disconnecting it and connecting it
//! again, removing the app, which asks first, and an app installed nowhere, whose row says so and installs it. The app
//! and its key are plainly made up.

#![cfg(test)]

use gpui_kit::{ElementId, SharedString, TestAppContext};
use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubAccountId, GithubApp};

use super::git::{default, in_git};
use super::git_app::{CODE, installed_as_set, making, on_adalabs};
use crate::support::{MENU, Stack, Ui};

/// How the hub asks GitHub where an app is installed, as the stand-in has the request.
const LOOKED: &str = "get /app/installations?";

/// The app, made as GitHub sent the person back with [`CODE`].
fn made(stack: &Stack) -> GithubApp {
    stack.client().github_app_make(GithubManifestCode::new(CODE)).unwrap()
}

/// The app's installation's account, as the hub has it.
fn installation(stack: &Stack) -> GithubAccountId {
    let accounts = stack.client().records().unwrap().github_accounts;
    accounts.into_iter().find(|account| account.installation.is_some()).unwrap().id
}

/// Whether the menu that's open has an item labelled `label`.
fn offers(ui: &mut Ui<'_>, label: &str) -> bool {
    let menu = ElementId::Name(MENU.into());
    ui.labelled(label).iter().any(|found| found.path().contains(&menu))
}

/// Opens the menu whose button is `button`.
fn open_menu(ui: &mut Ui<'_>, button: impl Into<SharedString>) {
    ui.click(ElementId::Name(button.into()));
    ui.wait("the menu", |ui| ui.has(MENU));
}

/// Whether the hub still holds a login for the installation `id`.
fn held(stack: &Stack, id: &GithubAccountId) -> bool {
    let accounts = stack.client().records().unwrap().github_accounts;
    accounts.iter().any(|account| &account.id == id && account.held_login.is_some())
}

#[gpui_kit::test]
fn an_installation_is_a_row_as_the_default_and_its_menu_disconnects_it_and_connects_it_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(making(vec![on_adalabs()]));
    made(&stack);
    let bot = installation(&stack);
    let mut ui = in_git(cx, &stack);
    let row = format!("git-account-{bot}");
    ui.wait("its row", |ui| ui.has(row.clone()));
    assert_eq!(default(&ui), Some(bot.clone()));
    assert!(ui.has(format!("{row}-default")));
    assert!(ui.shows_label(&row, "Installed on adalabs"));

    open_menu(&mut ui, format!("git-more-{bot}"));
    assert!(offers(&mut ui, "Disconnect") && offers(&mut ui, "Remove app"));
    assert!(!offers(&mut ui, "Make default"), "it's the default");
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("disconnect-github-account-act");

    ui.wait("agents blocked", |ui| ui.has("git-blocked") && ui.shows_label(&row, "Disconnected"));
    assert!(!held(&stack, &bot));
    open_menu(&mut ui, format!("git-more-{bot}"));
    assert!(offers(&mut ui, "Connect again") && offers(&mut ui, "Install on GitHub"));
    ui.click_menu_item("Connect again");

    ui.wait("it connected again", |ui| ui.shows_label(&row, "Installed on adalabs") && !ui.has("git-blocked"));
    assert!(held(&stack, &bot));
}

#[gpui_kit::test]
fn removing_an_app_asks_first_and_the_hub_forgets_it_leaving_its_installation_the_default(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(making(vec![on_adalabs()]));
    made(&stack);
    let bot = installation(&stack);
    let mut ui = in_git(cx, &stack);
    ui.wait("its row", |ui| ui.has(format!("git-more-{bot}")));

    open_menu(&mut ui, format!("git-more-{bot}"));
    ui.click_menu_item("Remove app");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("remove-github-app-keep");
    ui.wait_for_every_call();
    assert_eq!(stack.client().records().unwrap().github_apps.len(), 1, "nothing is removed without asking");

    open_menu(&mut ui, format!("git-more-{bot}"));
    ui.click_menu_item("Remove app");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("remove-github-app-act");

    ui.wait("the app gone", |ui| ui.view().records().github_apps().is_empty() && ui.has("git-blocked"));
    assert!(stack.client().records().unwrap().github_apps.is_empty());
    assert!(!held(&stack, &bot), "its installation's login went with it");
    assert_eq!(default(&ui), Some(bot), "and stays the default");
}

#[gpui_kit::test]
fn an_app_installed_nowhere_says_so_and_installing_it_from_its_row_waits_for_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (installed, script) = installed_as_set();
    let github = stack.start_hub_with_stand_in_github(script);
    let app = made(&stack);
    let mut ui = in_git(cx, &stack);
    let row = format!("git-app-{}", app.id);
    let looked = |_: &mut Ui<'_>| format!("GitHub was asked for its installations {} times", github.asked(LOOKED));
    ui.wait_saying("its row", |ui| ui.has(row.clone()), looked);
    assert!(ui.shows_label(&row, "Not installed"));

    open_menu(&mut ui, format!("git-more-app-{}", app.id));
    assert!(offers(&mut ui, "Check again") && offers(&mut ui, "Remove app"));
    ui.click_menu_item("Install on GitHub");

    assert_eq!(ui.cx.opened_url().as_deref(), Some("https://github.com/apps/ada-roundhouse/installations/new"));
    assert!(ui.has("git-check-again"), "the panel waits for it");
    installed.set(on_adalabs());
    ui.click("git-check-again");
    ui.wait("Done", |ui| ui.has("git-close"));
    assert!(!ui.has(row), "it's installed, so its installation's row has its place");
}
