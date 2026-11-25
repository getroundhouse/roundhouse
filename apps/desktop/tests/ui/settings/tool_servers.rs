//! MCP servers in Settings, against a real hub: one another client adds, adding one with a key and one with neither, a
//! name that's taken, and removing one, which asks first. Every server and key is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{SharedString, TestAppContext};
use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::open_settings;
use crate::support::{Stack, Ui, files_holding};

const URL: &str = "https://mcp.example.com/mcp";
const KEY: &str = "a-made-up-key";

/// Opens MCP servers in Settings, once the hub's records have arrived, so Add server is on.
fn in_servers<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("Add server to be on", |ui| ui.on("mcp-add"));
    ui
}

/// A server another client adds.
fn added_elsewhere(stack: &Stack, id: &str, name: &str, auth: ToolServerAuthGiven) -> ToolServerId {
    stack.client().tool_server_add(&id.into(), name, URL, auth).unwrap().id
}

fn with_key() -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(KEY) }
}

fn row(id: &ToolServerId) -> SharedString {
    format!("mcp-server-{id}").into()
}

fn more(id: &ToolServerId) -> SharedString {
    format!("mcp-more-{id}").into()
}

/// The server the app shows by this name.
fn named(ui: &Ui<'_>, name: &str) -> Option<ToolServer> {
    ui.view().records().tool_servers().iter().find(|server| server.name.as_str() == name).cloned()
}

/// Every server the hub has, by name, as it says when asked, rather than as the app shows it.
fn the_hubs(stack: &Stack) -> Vec<String> {
    stack.client().records().unwrap().tool_servers.iter().map(|server| server.name.to_string()).collect()
}

/// Types into the field `id`, in place of what's in it.
fn type_in(ui: &mut Ui<'_>, id: &'static str, typed: &str) {
    ui.click(id);
    ui.press("secondary-a");
    ui.input(typed);
}

/// One walk through adding servers, with one another client adds first, whose name is then taken.
#[gpui_kit::test]
fn a_server_is_added_with_a_key_only_the_hub_keeps_or_with_neither_and_a_name_thats_taken_says_so(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = in_servers(cx, &stack);
    assert!(ui.has("mcp-none-added"));
    let linear = added_elsewhere(&stack, "elsewhere", "Linear", ToolServerAuthGiven::Neither);
    ui.wait("the server another client added", |ui| ui.has(row(&linear)) && !ui.has("mcp-none-added"));

    ui.click("mcp-add");
    assert!(ui.off("mcp-add"), "while a panel is open");
    ui.click("mcp-use-key");
    ui.input(URL);
    type_in(&mut ui, "mcp-name", "linear");
    assert!(ui.off("mcp-panel-add"), "until there's a key");
    type_in(&mut ui, "mcp-key", KEY);
    ui.click("mcp-panel-add");
    ui.wait("the hub's refusal", |ui| ui.has("mcp-refused"));
    let refused = "Couldn't add it: agents would call linear and the MCP server Linear the same, Linear";
    assert!(ui.shows_label("mcp-panel", refused));
    assert!(ui.off("mcp-panel-add"), "a name that's taken: the key's field is empty");
    assert_eq!(the_hubs(&stack), ["Linear"], "a name that's taken: nothing is added");

    type_in(&mut ui, "mcp-key", KEY);
    type_in(&mut ui, "mcp-name", "Runbooks");
    ui.press("enter");
    ui.wait("the server with a key", |ui| named(ui, "Runbooks").is_some() && !ui.has("mcp-panel"));
    let runbooks = named(&ui, "Runbooks").unwrap();
    assert!(matches!(&runbooks.auth, ToolServerAuth::Key { header } if header.as_str() == "Authorization"));
    assert!(ui.shows_label(&row(&runbooks.id), "Key"));
    let everything = stack.dir.path();
    assert_eq!(files_holding(everything, KEY, "tool-servers"), Vec::<PathBuf>::new(), "nothing else has the key");
    assert_eq!(files_holding(everything, KEY, "").len(), 1, "the hub's tool servers do");

    ui.click("mcp-add");
    ui.click("mcp-use-neither");
    assert!(!ui.has("mcp-key") && !ui.has("mcp-header"));
    ui.input(URL);
    type_in(&mut ui, "mcp-name", "Docs");
    ui.click("mcp-panel-add");
    ui.wait("the server with neither", |ui| named(ui, "Docs").is_some() && !ui.has("mcp-panel"));
    assert_eq!(named(&ui, "Docs").unwrap().auth, ToolServerAuth::Neither);
    assert_eq!(the_hubs(&stack), ["Docs", "Linear", "Runbooks"]);
}

#[gpui_kit::test]
fn a_servers_menu_removes_it_once_asked_forgetting_its_key(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_servers(cx, &stack);
    added_elsewhere(&stack, "docs", "Docs", ToolServerAuthGiven::Neither);
    let server = added_elsewhere(&stack, "runbooks", "Runbooks", with_key());
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
