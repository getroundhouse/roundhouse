//! Signing in to an MCP server from Settings, against a real hub and a stand-in for the server and its authorization
//! server, with the test doing what the browser would: opening the page the app opens, where the stand-in has the person
//! approve or refuse, and going where that sends them back. Every server and sign-in is made up.

#![cfg(test)]

use std::path::PathBuf;
use std::time::Duration;

use desktop::browsing::{Fetched, fetch};
use gpui_kit::TestAppContext;
use hub::{ToolServerPage, ToolServerPerson, ToolServerRefreshing, ToolServerScript, ToolServerStandIn};
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInReturned, ToolSignInTo, redirect};
use protocol::model::{ToolServerAuth, ToolServerId, ToolSignInStatus};
use url::Url;

use super::git_app::label_of;
use super::open_settings;
use super::tool_servers::{in_servers, more, named, row, the_hubs, type_in};
use crate::support::{Stack, Ui, files_holding, hub_environment, wait_until};

const WORKS: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };
const NEEDS_SIGN_IN: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn };

/// The server and its authorization server, which serve until the test ends.
struct Serving {
    stand_in: ToolServerStandIn,
    _serving: tokio::runtime::Runtime,
}

impl Serving {
    fn with(script: ToolServerScript) -> Self {
        let serving = tokio::runtime::Runtime::new().unwrap();
        let stand_in = serving.block_on(ToolServerStandIn::with(script));
        Self { stand_in, _serving: serving }
    }

    fn url(&self) -> String {
        self.stand_in.server().as_str().to_string()
    }

    /// Where the person is sent back to from `page`, once they've done on it what the script says.
    fn sent_back_to(&self, page: &str) -> SentBack {
        SentBack { to: redirect_of(page), query: self.stand_in.visit(&ToolServerPage::new(page)) }
    }
}

/// Where a page sends the person back to.
struct SentBack {
    to: String,
    query: String,
}

impl SentBack {
    fn follow(&self) -> Fetched {
        fetch(&format!("{}?{}", self.to, self.query))
    }
}

fn redirect_of(page: &str) -> String {
    let page = Url::parse(page).unwrap();
    page.query_pairs().find(|(name, _)| name == "redirect_uri").unwrap().1.into_owned()
}

/// Waits until nothing listens where `page` sends the person back to.
fn until_stopped(page: &str) {
    let to = Url::parse(&redirect_of(page)).unwrap();
    let at = (to.host_str().unwrap().to_string(), to.port().unwrap());
    wait_until("the listener to stop", || std::net::TcpStream::connect(&at).is_err());
}

/// Opens the panel for signing in to a new server, and types its URL and its name.
fn fill_in(ui: &mut Ui<'_>, url: &str, name: &str) {
    ui.click("mcp-add");
    ui.click("mcp-sign-in");
    ui.input(url);
    assert!(ui.off("mcp-panel-add"), "until there's a name");
    type_in(ui, "mcp-name", name);
}

/// Waits for the browser to be opened on a page, other than `before`, and for the panel to wait there.
fn page_opened(ui: &mut Ui<'_>, before: Option<&str>) -> String {
    ui.wait("the server's page", |ui| ui.cx.opened_url().as_deref() != before && ui.has("mcp-open-page"));
    ui.cx.opened_url().unwrap()
}

/// What the panel says under its title.
fn says(ui: &mut Ui<'_>) -> String {
    label_of(ui, "mcp-panel-says")
}

fn refused(ui: &mut Ui<'_>) -> String {
    ui.wait("why", |ui| ui.has("mcp-refused"));
    label_of(ui, "mcp-refused")
}

fn auth_of(ui: &Ui<'_>, name: &str) -> Option<ToolServerAuth> {
    named(ui, name).map(|server| server.auth)
}

/// One walk through an approval: the panel while it waits, Settings closed and opened again meanwhile, a redirect that
/// isn't this sign-in's, and the one that is.
#[gpui_kit::test]
fn a_server_is_added_once_its_sign_in_is_approved_in_the_browser_whatever_the_window_shows(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, &serving.url(), "Linear");
    assert_eq!(says(&mut ui), "Your browser opens on its own page. Every agent has its tools once you've signed in.");

    ui.click("mcp-panel-add");

    let page = page_opened(&mut ui, None);
    let machine = if cfg!(target_os = "macos") { "Mac" } else { "computer" };
    let approve = format!(
        "Approve Roundhouse on 127.0.0.1, in the browser on this {machine}. Roundhouse never sees your password."
    );
    assert_eq!(says(&mut ui), approve);
    assert!(ui.off("mcp-add"), "while a panel is open");
    let back = serving.sent_back_to(&page);
    let another = SentBack { to: back.to.clone(), query: "code=a-made-up-code&state=another".to_string() };
    assert_eq!(another.follow().status, 400);
    ui.click("settings-back");
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("its panel again", |ui| ui.has("mcp-open-page"));
    assert_eq!(the_hubs(&stack), Vec::<String>::new(), "nothing is added until it's approved");
    ui.click("mcp-open-page");
    assert_eq!(ui.cx.opened_url(), Some(page.clone()));

    let answered = back.follow();

    assert_eq!(answered.status, 200);
    assert!(answered.body.contains("Go back to Roundhouse"), "{}", answered.body);
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
    let linear = named(&ui, "Linear").unwrap();
    assert!(!ui.has(format!("{}-says", row(&linear.id))), "one that works says nothing more");
    assert_eq!(serving.stand_in.asked_for("POST /token").len(), 1);
    until_stopped(&page);
    let code = back.query.split('&').find_map(|said| said.strip_prefix("code=")).unwrap();
    assert_eq!(files_holding(stack.dir.path(), code, ""), Vec::<PathBuf>::new(), "what came back isn't kept");
}

#[gpui_kit::test]
fn a_sign_in_thats_refused_says_so_and_can_be_changed_and_tried_again(cx: &mut TestAppContext) {
    let script = ToolServerScript { person: ToolServerPerson::Refuses, ..ToolServerScript::default() };
    let (stack, serving) = (Stack::new(), Serving::with(script));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, "mcp.example.com/mcp", "Linear");
    ui.press("enter");
    let not_a_url = "Couldn't sign in: an MCP server's URL starts with http:// or https://";
    assert_eq!(refused(&mut ui), not_a_url, "before the hub asks anyone");
    type_in(&mut ui, "mcp-url", &serving.url());
    ui.click("mcp-panel-add");
    let page = page_opened(&mut ui, None);

    let answered = serving.sent_back_to(&page).follow();

    assert!(answered.body.contains("Go back to Roundhouse, which says why."), "{}", answered.body);
    assert_eq!(refused(&mut ui), "Couldn't sign in: the sign-in was refused");
    assert_eq!(the_hubs(&stack), Vec::<String>::new());
    until_stopped(&page);
    serving.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click("mcp-panel-add");
    let again = page_opened(&mut ui, Some(&page));
    serving.sent_back_to(&again).follow();
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
}

#[gpui_kit::test]
fn a_server_with_no_way_to_sign_in_says_so_and_takes_a_key_in_its_place(cx: &mut TestAppContext) {
    let script = ToolServerScript { client_documents: false, ..ToolServerScript::default() };
    let (stack, serving) = (Stack::new(), Serving::with(script));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, &serving.url(), "Runbooks");

    ui.click("mcp-panel-add");

    ui.wait("the offer of a key", |ui| ui.has("mcp-key-instead"));
    let says = says(&mut ui);
    assert!(says.starts_with("Roundhouse can't sign in to this server."), "{says}");
    assert!(says.ends_with("Nothing was added. It may take a key."), "{says}");
    assert_eq!(ui.cx.opened_url(), None);
    ui.click("mcp-key-instead");
    ui.input("a-made-up-key");
    ui.press("enter");
    ui.wait("the server", |ui| named(ui, "Runbooks").is_some() && !ui.has("mcp-panel"));
    let runbooks = named(&ui, "Runbooks").unwrap();
    assert_eq!(runbooks.url.as_str(), serving.url(), "what was typed was kept");
    assert!(matches!(runbooks.auth, ToolServerAuth::Key { .. }));
}

#[gpui_kit::test]
fn cancelling_a_sign_in_that_waits_puts_its_panel_away_and_nothing_listens(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), Serving::with(ToolServerScript::default()));
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, &serving.url(), "Linear");
    ui.click("mcp-panel-add");
    let page = page_opened(&mut ui, None);

    ui.click("mcp-cancel");

    assert!(!ui.has("mcp-panel"));
    until_stopped(&page);
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack), Vec::<String>::new());
    assert!(ui.on("mcp-add"));
}

/// Signs in to the stand-in's server as `Linear`, from another client.
fn signed_in_elsewhere(stack: &Stack, serving: &Serving) -> ToolServerId {
    let (client, id) = (stack.client(), client_core::new_tool_sign_in_id());
    let to = ToolSignInTo::New { id: "t".into(), name: "Linear".into(), url: serving.url() };
    let begun = client.tool_sign_in_start(&id, to, redirect(17867)).unwrap();
    let ToolSignInBegun::Page { page } = begun else { panic!("{begun:?}") };
    let returned = serving.stand_in.visit(&ToolServerPage::new(page.as_str()));
    client.tool_sign_in_finish(&id, ToolSignInReturned::new(returned)).unwrap().id
}

/// Its tokens end sooner than the hub renews one, so each check does, until the authorization server refuses one.
#[gpui_kit::test]
fn a_server_that_needs_it_is_signed_in_to_again_from_its_menu_and_a_failure_is_tried_again(cx: &mut TestAppContext) {
    let (timers, short) =
        hub::Timers { check_tool_sign_ins_every: Duration::from_millis(50), ..hub::Timers::FOR_TESTS }.env();
    let stack = Stack::in_environment(hub_environment().set(timers, short));
    let script = ToolServerScript { lasts: Some(60), ..ToolServerScript::default() };
    let serving = Serving::with(script);
    let mut ui = in_servers(cx, &stack);
    let linear = signed_in_elsewhere(&stack, &serving);
    ui.wait("the server", |ui| auth_of(ui, "Linear") == Some(WORKS));
    let refuses = ToolServerRefreshing::Refused("invalid_grant");
    serving.stand_in.change(|script| script.refreshing = refuses);
    ui.wait("it to need signing in to", |ui| auth_of(ui, "Linear") == Some(NEEDS_SIGN_IN));
    assert!(ui.shows_label(&row(&linear), "Needs you to sign in"));
    serving.stand_in.change(|script| {
        (script.refreshing, script.person) = (ToolServerRefreshing::Works, ToolServerPerson::Refuses);
    });

    ui.click(more(&linear));
    ui.click_menu_item("Sign in again");

    let page = page_opened(&mut ui, None);
    serving.sent_back_to(&page).follow();
    assert_eq!(refused(&mut ui), "Couldn't sign in: the sign-in was refused");
    assert!(ui.has("mcp-close"));
    serving.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click("mcp-try-again");
    let again = page_opened(&mut ui, Some(&page));
    serving.sent_back_to(&again).follow();
    ui.wait("its sign-in to work", |ui| auth_of(ui, "Linear") == Some(WORKS) && !ui.has("mcp-panel"));
    assert_eq!(named(&ui, "Linear").unwrap().id, linear, "the same server");
    assert!(!ui.has(format!("{}-says", row(&linear))));
}
