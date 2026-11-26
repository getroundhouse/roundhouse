//! MCP servers in Settings, against a real hub, and a stand-in for one that's asked for its tools as it's added: one
//! another client adds, adding one with a key and one with neither, a name that's taken, changing the way before one is
//! added, the hub going while one is typed, and removing one, which asks first. What Escape closes, against a fake hub,
//! since it asks the hub nothing. Every server and key is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use client_core::fake_hub::World;
use desktop::app::AppView;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use hub::{ToolServerAsks, ToolServerScript};
use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::git_app::label_of;
use super::tool_server_check::filled_in_on;
use super::{open_settings, settings_showing};
use crate::support::{MENU, Serving, Stack, Ui, files_holding};

pub(super) const URL: &str = "https://mcp.example.com/mcp";
pub(super) const KEY: &str = "a-made-up-key";

/// The dropdown's ways, as its menu lists them.
pub(super) const SIGN_IN_WAY: &str = "Sign in, In your browser";
pub(super) const KEY_WAY: &str = "Key, Sent with every request";
pub(super) const NONE_WAY: &str = "None, Anyone can use it";

/// What the dropdown says while a sign-in is chosen.
pub(super) const SIGN_IN_CHOSEN: &str = "Authentication: Sign in";

/// Opens MCP servers in Settings, once the hub's records have arrived, so Add server is on.
pub(super) fn in_servers<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("Add server to be on", |ui| ui.on("mcp-add"));
    ui
}

/// A server another client adds.
pub(super) fn added_elsewhere(
    stack: &Stack,
    id: &str,
    name: &str,
    url: &str,
    auth: ToolServerAuthGiven,
) -> ToolServerId {
    stack.client().tool_server_add(&id.into(), name, url, auth).unwrap().id
}

/// A server that anyone can use, with a key or with none.
pub(super) fn open_to_anyone() -> Serving {
    Serving::with(ToolServerScript { asks: ToolServerAsks::Nothing, ..ToolServerScript::default() })
}

pub(super) fn with_key() -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(KEY) }
}

pub(super) fn row(id: &ToolServerId) -> SharedString {
    format!("mcp-server-{id}").into()
}

pub(super) fn more(id: &ToolServerId) -> SharedString {
    format!("mcp-more-{id}").into()
}

/// The server the app shows by this name.
pub(super) fn named(ui: &Ui<'_>, name: &str) -> Option<ToolServer> {
    ui.view().records().tool_servers().iter().find(|server| server.name.as_str() == name).cloned()
}

/// Every server the hub has, by name, as it says when asked, rather than as the app shows it.
pub(super) fn the_hubs(stack: &Stack) -> Vec<String> {
    stack.client().records().unwrap().tool_servers.iter().map(|server| server.name.to_string()).collect()
}

/// Types into the field `id`, in place of what's in it.
pub(super) fn type_in(ui: &mut Ui<'_>, id: &'static str, typed: &str) {
    ui.click(id);
    ui.press("secondary-a");
    ui.input(typed);
}

/// Opens the panel for adding a server, with the cursor in its name, and types its name and its URL.
pub(super) fn fill_in(ui: &mut Ui<'_>, name: &str, url: &str) {
    ui.click("mcp-add");
    assert_eq!(label_of(ui, "mcp-way"), SIGN_IN_CHOSEN);
    ui.input(name);
    assert!(ui.off("mcp-panel-go"), "until there's a URL");
    type_in(ui, "mcp-url", url);
}

/// Chooses how the server knows the person, from the dropdown.
pub(super) fn choose(ui: &mut Ui<'_>, way: &str) {
    ui.click("mcp-way");
    ui.click_menu_item(way);
}

/// One walk through adding servers, with one another client adds first, whose name is then taken.
#[gpui_kit::test]
fn a_server_is_added_with_a_key_only_the_hub_keeps_or_with_neither_and_a_name_thats_taken_says_so(
    cx: &mut TestAppContext,
) {
    let (stack, serving) = (Stack::new(), open_to_anyone());
    let mut ui = in_servers(cx, &stack);
    assert!(ui.has("mcp-none-added"));
    let linear = added_elsewhere(&stack, "elsewhere", "Linear", URL, ToolServerAuthGiven::Neither);
    ui.wait("the server another client added", |ui| ui.has(row(&linear)) && !ui.has("mcp-none-added"));

    fill_in(&mut ui, "linear", &serving.url());
    assert!(ui.off("mcp-add"), "while a panel is open");
    choose(&mut ui, KEY_WAY);
    assert!(ui.off("mcp-panel-go"), "until there's a key");
    ui.input(KEY);
    ui.click("mcp-panel-go");
    ui.wait("the hub's refusal", |ui| ui.has("mcp-refused"));
    let refused = "Couldn't add it: agents would call linear and the MCP server Linear the same, Linear";
    assert!(ui.shows_label("mcp-panel", refused));
    assert!(serving.stand_in.asked().is_empty(), "a name that's taken: nothing is asked");
    assert_eq!(the_hubs(&stack), ["Linear"], "a name that's taken: nothing is added");

    type_in(&mut ui, "mcp-name", "Runbooks");
    ui.press("enter");
    ui.wait("the server with a key", |ui| named(ui, "Runbooks").is_some() && !ui.has("mcp-panel"));
    let runbooks = named(&ui, "Runbooks").unwrap();
    assert!(matches!(&runbooks.auth, ToolServerAuth::Key { header } if header.as_str() == "Authorization"));
    assert!(ui.shows_label(&row(&runbooks.id), "Key"));
    let everything = stack.dir.path();
    assert_eq!(files_holding(everything, KEY, "tool-servers"), Vec::<PathBuf>::new(), "nothing else has the key");
    assert_eq!(files_holding(everything, KEY, "").len(), 1, "the hub's tool servers do");

    fill_in(&mut ui, "Docs", &serving.url());
    choose(&mut ui, NONE_WAY);
    assert!(!ui.has("mcp-key") && !ui.has("mcp-header"));
    ui.click("mcp-panel-go");
    ui.wait("the server with neither", |ui| named(ui, "Docs").is_some() && !ui.has("mcp-panel"));
    assert_eq!(named(&ui, "Docs").unwrap().auth, ToolServerAuth::Neither);
    assert_eq!(the_hubs(&stack), ["Docs", "Linear", "Runbooks"]);
}

/// The name that's kept is taken, so the hub's refusal of it says it was.
#[gpui_kit::test]
fn changing_how_a_server_knows_you_keeps_its_name_and_url_and_drops_the_key_and_a_refusal(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), open_to_anyone());
    let mut ui = in_servers(cx, &stack);
    let linear = added_elsewhere(&stack, "elsewhere", "Linear", URL, ToolServerAuthGiven::Neither);
    ui.wait("the server another client added", |ui| ui.has(row(&linear)));
    fill_in(&mut ui, "linear", &serving.url());
    choose(&mut ui, KEY_WAY);
    ui.input(KEY);
    assert!(ui.on("mcp-panel-go"));

    choose(&mut ui, SIGN_IN_WAY);

    assert!(!ui.has("mcp-key") && !ui.has("mcp-header"));
    assert!(ui.on("mcp-panel-go"), "with the name and the URL");
    choose(&mut ui, KEY_WAY);
    assert!(ui.off("mcp-panel-go"), "the key went");
    ui.input(KEY);
    ui.press("enter");
    ui.wait("the hub's refusal", |ui| ui.has("mcp-refused"));
    choose(&mut ui, NONE_WAY);
    assert!(!ui.has("mcp-refused"), "it was of the way before");
    ui.click("mcp-panel-go");
    ui.wait("the same refusal", |ui| ui.has("mcp-refused"));
    let refused = "Couldn't add it: agents would call linear and the MCP server Linear the same, Linear";
    assert_eq!(label_of(&mut ui, "mcp-refused"), refused);
    type_in(&mut ui, "mcp-name", "Docs");
    ui.press("enter");
    ui.wait("the server", |ui| named(ui, "Docs").is_some() && !ui.has("mcp-panel"));
    let docs = named(&ui, "Docs").unwrap();
    assert_eq!((docs.url.as_str(), docs.auth), (serving.url().as_str(), ToolServerAuth::Neither));
}

/// The hub stops while a key is typed, and the app can't start another, so the panel waits, with what's typed, until
/// there's a hub to ask again.
#[gpui_kit::test]
fn a_panel_keeps_whats_typed_while_theres_no_hub_and_adds_it_once_theres_one_again(cx: &mut TestAppContext) {
    let (mut stack, serving) = (Stack::new(), open_to_anyone());
    // Started for the app to connect to, since the app then has no daemon to start one with.
    stack.client();
    let daemon = stack.without_a_daemon();
    let mut ui = in_servers(cx, &stack);
    fill_in(&mut ui, "Runbooks", &serving.url());
    choose(&mut ui, KEY_WAY);
    ui.input(KEY);
    assert!(ui.on("mcp-panel-go"));

    ui.lose_the_hub(&stack);

    ui.wait("no hub to ask", |ui| ui.off("mcp-panel-go"));
    assert!(ui.has("mcp-panel"), "the panel stays");
    local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    // As Try again does, which Settings covers.
    ui.cx.update_entity(&ui.view.clone(), AppView::connect);
    ui.wait("a hub to ask", |ui| ui.on("mcp-panel-go"));
    ui.click("mcp-panel-go");
    ui.wait("it to be added", |ui| named(ui, "Runbooks").is_some() && !ui.has("mcp-panel"));
    assert_eq!(files_holding(stack.dir.path(), KEY, "").len(), 1, "with the key that was typed");
}

#[gpui_kit::test]
fn a_servers_menu_removes_it_once_asked_forgetting_its_key(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_servers(cx, &stack);
    added_elsewhere(&stack, "docs", "Docs", URL, ToolServerAuthGiven::Neither);
    let server = added_elsewhere(&stack, "runbooks", "Runbooks", URL, with_key());
    ui.wait("its row", |ui| ui.has(more(&server)));

    let keys = stack.hub.paths.data_dir.join("tool-servers");
    ui.click(more(&server));
    ui.click_menu_item("Remove");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("remove-mcp-server-keep");
    ui.wait_for_every_call();
    assert_eq!(files_holding(&keys, KEY, "").len(), 1, "nothing is forgotten without asking");
    ui.click(more(&server));
    ui.click_menu_item("Remove");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("remove-mcp-server-act");

    ui.wait("its row gone", |ui| !ui.has(row(&server)));
    assert_eq!(the_hubs(&stack), ["Docs"]);
    assert!(files_holding(&keys, KEY, "").is_empty(), "the hub forgot its key");
}

/// One walk through what Escape closes in Settings, from the inside out: the dropdown's menu, then the panel, with the
/// cursor in its key, as its Cancel does, then Settings, once no panel is open.
#[gpui_kit::test]
fn escape_closes_a_menu_then_its_panel_then_settings(cx: &mut TestAppContext) {
    let (_fake, mut ui) = filled_in_on(cx, World::default());
    ui.click("mcp-way");
    ui.wait("the menu", |ui| ui.has(MENU));

    ui.press("escape");

    ui.wait("the menu to close", |ui| !ui.has(MENU));
    assert!(ui.has("mcp-panel"), "the menu: the panel stays open");
    ui.click("mcp-key");
    ui.press("escape");
    assert!(!ui.has("mcp-panel"), "the key's field: the panel closes");
    assert!(settings_showing(&mut ui), "the key's field: Settings stays");
    ui.click("mcp-add");
    ui.press("escape");
    assert!(!ui.has("mcp-panel"), "the name's field: the panel closes");
    ui.press("escape");
    assert!(!settings_showing(&mut ui), "no panel: Settings closes");
}
