//! Making an app of your own from Git in Settings, against a real hub that calls a stand-in for GitHub, with the test
//! doing what the browser and GitHub would with the page the app serves: each step's words and buttons, the page
//! posting the hub's manifest, GitHub sending the person back with a code, which the hub makes into the app, installing
//! it, which the app asks the hub to look for as its window comes back, a code GitHub no longer takes, and starting
//! again. The app and its key are plainly made up.

#![cfg(test)]

use desktop::browsing::fetch;
use desktop::github_app_browsing::{come_back, form};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use hub::{GithubAnswer, GithubInstalled, GithubScript};
use protocol::model::{GithubAccount, GithubLoginKind};

use super::git::{default, in_git};
use crate::support::{Stack, Ui};

/// The code GitHub sends the person back with, once they've made the app, which it makes into `ada-roundhouse`.
pub(super) const CODE: &str = "a-made-up-code";

/// GitHub, which makes the app 9, `ada-roundhouse`, under adalabs from [`CODE`], names its bot, lists where it's
/// installed in turn as `listed` says, and makes its installation there, 70, a token.
pub(super) fn making(listed: Vec<GithubAnswer>) -> GithubScript {
    GithubScript {
        conversions: vec![(CODE, GithubAnswer::app_made(9, "ada-roundhouse", (2, "adalabs")))],
        named: vec![("ada-roundhouse[bot]", GithubAnswer::user(41, "ada-roundhouse[bot]", None))],
        installations: listed,
        minted: vec![(70, vec![GithubAnswer::installation_token("ghs_made_up_for_a_test", "2099-01-01T00:00:00Z")])],
        ..GithubScript::default()
    }
}

/// A script for making the app, with GitHub listing it as installed nowhere until the test sets where it is, as it is
/// when the hub looks, since the hub looks by itself as it checks its logins, as well as when it's made and when it's
/// checked again, so answers in turn could go to a look they aren't for.
pub(super) fn installed_as_set() -> (GithubInstalled, GithubScript) {
    let installed = GithubInstalled::default();
    (installed.clone(), GithubScript { installed: Some(installed), ..making(Vec::new()) })
}

pub(super) fn nowhere() -> GithubAnswer {
    GithubAnswer::installations(&[])
}

pub(super) fn on_adalabs() -> GithubAnswer {
    GithubAnswer::installations(&[(70, 2, "adalabs")])
}

/// What the element `id` is labelled, as it tells assistive apps.
pub(super) fn label_of(ui: &mut Ui<'_>, id: &'static str) -> String {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).label().unwrap().to_string()).unwrap()
}

/// What the panel says under its title.
fn says(ui: &mut Ui<'_>) -> String {
    label_of(ui, "git-panel-says")
}

/// What the panel says the hub, or GitHub, refused.
fn refused(ui: &mut Ui<'_>) -> String {
    label_of(ui, "git-refused")
}

/// What each step's line says, and whether it's done.
fn steps(ui: &mut Ui<'_>) -> [String; 3] {
    ["git-app-step-1", "git-app-step-2", "git-app-step-3"].map(|id| label_of(ui, id))
}

/// Waits until nothing listens where the page at `page` was served.
fn until_stopped(page: &str) {
    let address = page.strip_prefix("http://").unwrap().split_once('/').unwrap().0.to_string();
    crate::support::wait_until("the listener to stop", || std::net::TcpStream::connect(&address).is_err());
}

/// The account that's the app's installation, once the app has it.
fn installation(ui: &Ui<'_>) -> Option<GithubAccount> {
    let view = ui.view();
    let records = view.records();
    let installed = records.github_accounts().into_iter().find(|a| a.installation.is_some() && a.held_login.is_some());
    installed.cloned()
}

/// Opens the panel for making an app, from Connect account.
fn making_an_app<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = in_git(cx, stack);
    ui.click("git-connect");
    ui.click("git-make-app");
    ui.wait("the panel", |ui| ui.has("git-make-on-github"));
    ui
}

/// Makes it on GitHub, as named, and waits for the browser to be opened on the app's page.
fn on_github(ui: &mut Ui<'_>) -> String {
    let before = ui.cx.opened_url();
    ui.click("git-make-on-github");
    ui.wait("GitHub's page", |ui| ui.cx.opened_url() != before && ui.has("git-open-github"));
    ui.cx.opened_url().unwrap()
}

#[gpui_kit::test]
fn an_app_is_made_on_github_and_connected_as_the_window_comes_back_once_its_installed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (installed, script) = installed_as_set();
    let _github = stack.start_hub_with_stand_in_github(script);
    let mut ui = making_an_app(cx, &stack);
    assert_eq!(says(&mut ui), "Agents act as its bot, rather than as you. Only the hub holds its key.");
    let first = "Choose where to make it, and its name";
    assert_eq!(steps(&mut ui), [first, "Make it on GitHub", "Install it on the repositories agents should reach"]);
    assert!(ui.has("git-app-under-account") && ui.has("git-app-name") && !ui.has("git-app-organization"));
    assert!(ui.off("git-make-on-github"), "with no name, as no account or login says one");
    ui.input("ada-roundhouse");

    let page = on_github(&mut ui);

    assert!(page.starts_with("http://127.0.0.1:"), "{page}");
    let posted = form(&fetch(&page).body);
    assert!(posted.action.starts_with("https://github.com/settings/apps/new?state="), "{}", posted.action);
    assert_eq!(
        (posted.manifest["name"].as_str(), posted.manifest["public"].as_bool()),
        (Some("ada-roundhouse"), Some(false))
    );
    let [chosen, making, _] = steps(&mut ui);
    assert_eq!(
        (chosen.as_str(), making.as_str()),
        ("Done: Under your account, as ada-roundhouse", "Make it on GitHub")
    );
    assert!(ui.has("git-cancel"));

    assert_eq!(come_back(&page, CODE).status, 200);

    ui.wait("the install step", |ui| ui.has("git-install"));
    assert_eq!(ui.view().records().github_apps().len(), 1);
    // The hub tells of the app as it's made, then asks GitHub for its bot, so the bot's name comes in a change of its
    // own, and can reach the app after the install step shows.
    let named = "Agents act as its bot, ada-roundhouse[bot], rather than as you. Only the hub holds its key.";
    ui.wait("the bot's name", |ui| says(ui) == named);
    assert_eq!(steps(&mut ui)[1], "Done: Made ada-roundhouse on GitHub");
    assert!(installation(&ui).is_none(), "GitHub listed it nowhere as it was made");
    installed.set(on_adalabs());
    ui.click("git-install");
    assert_eq!(ui.cx.opened_url().as_deref(), Some("https://github.com/apps/ada-roundhouse/installations/new"));

    ui.send_to_back();
    ui.bring_to_front();

    ui.wait("Done", |ui| ui.has("git-close"));
    let account = installation(&ui).unwrap();
    // What GitHub calls its bot comes apart from its record, once the hub has asked.
    ui.wait("its bot's username", |ui| {
        ui.view().records().github_username(&account).to_string() == "ada-roundhouse[bot]"
    });
    assert_eq!(account.held_login.map(|held| held.how), Some(GithubLoginKind::App));
    // Which account is the default is on the hub's settings, a record of their own, which reaches the app apart from
    // the account's, so it's waited for rather than read as soon as the account is there.
    ui.wait("the first account to be the default", |ui| default(ui) == Some(account.id.clone()));
    // Where it's installed is GitHub's to say, which the hub keeps apart from the account's record, so its name comes
    // in a change of its own, after the account's, as the bot's name comes after the app's.
    let installed = "ada-roundhouse[bot] is installed on adalabs. It's the default, so agents push as it wherever Roundhouse sets up GitHub.";
    ui.wait("where it's installed", |ui| says(ui) == installed);
    assert_eq!(steps(&mut ui)[2], "Done: Installed on adalabs");

    ui.click("git-close");

    assert!(!ui.has("git-panel"));
    assert!(ui.shows_label(&format!("git-account-{}", account.id), "Installed on adalabs"));
}

#[gpui_kit::test]
fn an_app_under_an_organization_is_made_at_its_form_and_check_again_asks_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (installed, script) = installed_as_set();
    let _github = stack.start_hub_with_stand_in_github(script);
    let mut ui = making_an_app(cx, &stack);
    ui.input("ada-roundhouse");
    ui.click("git-app-under-organization");
    assert!(ui.has("git-app-organization"));
    assert!(ui.off("git-make-on-github"), "until there's an organization's name");
    ui.input("ada labs");
    ui.click("git-make-on-github");
    ui.wait("why", |ui| ui.has("git-refused"));
    assert!(refused(&mut ui).starts_with("ada labs isn't an organization's name."), "{}", refused(&mut ui));
    assert!(!ui.has("git-open-github"), "it goes no further");
    ui.click("git-app-organization");
    ui.press("secondary-a");
    ui.input("adalabs");

    let page = on_github(&mut ui);

    let posted = form(&fetch(&page).body);
    assert!(posted.action.starts_with("https://github.com/organizations/adalabs/settings/apps/new?state="));
    assert_eq!(steps(&mut ui)[0], "Done: Under adalabs, as ada-roundhouse");
    come_back(&page, CODE);
    ui.wait("the install step", |ui| ui.has("git-check-again"));

    installed.set(on_adalabs());
    ui.click("git-check-again");

    ui.wait("Done", |ui| ui.has("git-close"));
    assert!(installation(&ui).is_some());
}

#[gpui_kit::test]
fn a_code_github_no_longer_takes_says_why_and_starting_again_makes_the_app(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(making(vec![nowhere()]));
    let mut ui = making_an_app(cx, &stack);
    ui.input("ada-roundhouse");
    let first = on_github(&mut ui);

    come_back(&first, "a-code-github-never-made");

    ui.wait("why", |ui| ui.has("git-refused") && ui.has("git-start-again"));
    assert!(refused(&mut ui).starts_with("GitHub no longer takes the code it sent you back with"));
    assert!(!ui.has("git-try-again"), "the same code would fare no better");
    assert!(ui.view().records().github_apps().is_empty(), "nothing was made");
    until_stopped(&first);

    ui.click("git-start-again");

    ui.wait("the first step", |ui| ui.on("git-make-on-github"));
    let again = on_github(&mut ui);
    assert_ne!(again, first, "a new listener, with a state of its own");
    come_back(&again, CODE);
    ui.wait("the install step", |ui| ui.has("git-install"));
    assert_eq!(ui.view().records().github_apps().len(), 1);
}

#[gpui_kit::test]
fn cancelling_while_github_has_the_form_stops_the_listener(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(making(vec![nowhere()]));
    let mut ui = making_an_app(cx, &stack);
    ui.input("ada-roundhouse");
    let page = on_github(&mut ui);

    ui.click("git-cancel");

    assert!(!ui.has("git-panel"));
    until_stopped(&page);
    assert!(ui.view().records().github_apps().is_empty());
}
