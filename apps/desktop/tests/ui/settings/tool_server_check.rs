//! A server asked for its tools as it's added or edited in Settings: what the panel says and does while it's asked and
//! once it hasn't listed them, and where its key is meanwhile. Against a real hub and a stand-in for the server, and
//! against a fake hub for one from before checks. Every server and key is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use client_core::fake_hub::{ToolServerCheckWorld, World};
use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use hub::{ToolServerAsks, ToolServerGate, ToolServerScript};
use protocol::client::tool_server_checks::{NoToolList, ToolServerChecked};
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::tool_server;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerName};

use super::git_app::label_of;
use super::open_settings;
use super::tool_server_edit::{TOOLS, edit};
use super::tool_servers::{
    KEY, KEY_WAY, NONE_WAY, added_elsewhere, choose, fill_in, in_servers, more, named, open_to_anyone, the_hubs,
    type_in, with_key,
};
use super::tool_sign_in::{WORKS, refused, says};
use crate::support::{FakeHub, Serving, Stack, Ui, files_holding};

const REFUSED_KEY: &str = "Runbooks refused this key.";

/// What its main button says, and whether there's one beside it that goes on anyway.
fn buttons(ui: &mut Ui<'_>) -> (String, Option<String>) {
    let goes = label_of(ui, "mcp-panel-go");
    (goes, ui.has("mcp-anyway").then(|| label_of(ui, "mcp-anyway")))
}

fn added(ui: &mut Ui<'_>, name: &str) {
    ui.wait("it to be added or saved", |ui| named(ui, name).is_some() && !ui.has("mcp-panel"));
}

/// A server that refuses any key, until its script says otherwise.
fn refusing() -> Serving {
    Serving::with(ToolServerScript { asks: ToolServerAsks::Answers(401), ..ToolServerScript::default() })
}

/// Opens the panel for adding `serving` as Runbooks, with a key.
fn fill_in_with_a_key(ui: &mut Ui<'_>, serving: &Serving) {
    fill_in(ui, "Runbooks", &serving.url());
    choose(ui, KEY_WAY);
    ui.input(KEY);
}

/// Cancelled, then as a field changes, then as Settings closes, and the last time let go on.
#[gpui_kit::test]
fn a_server_is_asked_for_its_tools_before_its_added_and_nothing_is_added_once_thats_let_go_of(cx: &mut TestAppContext) {
    let gate = ToolServerGate::closed();
    let script = ToolServerScript {
        asks: ToolServerAsks::Nothing,
        server_after: Some(gate.clone()),
        ..ToolServerScript::default()
    };
    let (stack, serving) = (Stack::new(), Serving::with(script));
    let mut ui = in_servers(cx, &stack);
    let url = serving.url();
    let host = url.strip_prefix("http://").unwrap().split('/').next().unwrap();
    let says = format!("Asking {host} for its tools");
    let asks = |ui: &mut Ui<'_>| {
        ui.click("mcp-panel-go");
        assert_eq!(label_of(ui, "mcp-asking"), says);
        assert!(ui.off("mcp-panel-go"));
        assert_eq!(buttons(ui), ("Add".to_string(), None));
    };

    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    asks(&mut ui);
    ui.click("mcp-cancel");
    assert!(!ui.has("mcp-panel") && ui.on("mcp-add"));

    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    asks(&mut ui);
    type_in(&mut ui, "mcp-name", "Guides");
    assert!(!ui.has("mcp-asking") && ui.on("mcp-panel-go"));
    gate.open();
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack), Vec::<String>::new(), "an answer lands on no panel but its own, as it was");
    assert!(ui.on("mcp-panel-go"));

    let gate = ToolServerGate::closed();
    serving.stand_in.change(|script| script.server_after = Some(gate.clone()));
    asks(&mut ui);
    ui.click("settings-back");
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    assert!(!ui.has("mcp-panel"));
    gate.open();
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack), Vec::<String>::new());

    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    added(&mut ui, "Docs");
    assert_eq!(the_hubs(&stack), ["Docs"]);
}

/// Tried again with Enter, then changed, then added anyway. What's typed of the key stays throughout, is sent as it's
/// added, and is nowhere but with the hub after.
#[gpui_kit::test]
fn a_server_that_refuses_its_key_says_so_and_its_tried_again_changed_or_added_anyway(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), refusing());
    let mut ui = in_servers(cx, &stack);
    fill_in_with_a_key(&mut ui, &serving);

    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), REFUSED_KEY);
    let asked_again = ("Try again".to_string(), Some("Add anyway".to_string()));
    assert_eq!(buttons(&mut ui), asked_again);
    let asked = || serving.stand_in.asked().len();
    let once = asked();
    ui.click("mcp-key");
    ui.press("enter");
    ui.wait("it to be asked again", |_| asked() == 2 * once);
    assert_eq!(refused(&mut ui), REFUSED_KEY);
    assert_eq!(buttons(&mut ui), asked_again);

    for field in ["mcp-name", "mcp-url", "mcp-header", "mcp-key"] {
        ui.click(field);
        ui.press("end");
        ui.input("s");
        assert!(!ui.has("mcp-refused"), "{field}: it's of what there was");
        assert_eq!(buttons(&mut ui), ("Add".to_string(), None), "{field}");
        ui.press("backspace");
        ui.press("enter");
        assert_eq!(refused(&mut ui), REFUSED_KEY, "{field}: as it was, with its key");
    }
    choose(&mut ui, NONE_WAY);
    assert_eq!((ui.has("mcp-refused"), buttons(&mut ui)), (false, ("Add".to_string(), None)));
    choose(&mut ui, KEY_WAY);
    ui.input(KEY);
    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), REFUSED_KEY);
    assert_eq!((the_hubs(&stack), files_holding(stack.dir.path(), KEY, "")), (vec![], Vec::<PathBuf>::new()));

    ui.click("mcp-anyway");

    added(&mut ui, "Runbooks");
    let everything = stack.dir.path();
    assert_eq!(files_holding(everything, KEY, "tool-servers"), Vec::<PathBuf>::new(), "nothing else has the key");
    assert_eq!(files_holding(everything, KEY, "").len(), 1, "the hub's tool servers do");
}

#[gpui_kit::test]
fn a_server_that_takes_its_key_the_next_time_is_added_and_a_key_goes_as_its_panel_does(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), refusing());
    let mut ui = in_servers(cx, &stack);
    fill_in_with_a_key(&mut ui, &serving);
    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), REFUSED_KEY);

    ui.click("mcp-cancel");
    fill_in(&mut ui, "Runbooks", &serving.url());
    choose(&mut ui, KEY_WAY);

    assert!(ui.off("mcp-panel-go"), "what was typed of the key went");
    ui.input(KEY);
    ui.press("enter");
    assert_eq!(refused(&mut ui), REFUSED_KEY);
    serving.stand_in.change(|script| script.asks = ToolServerAsks::Nothing);
    ui.click("mcp-panel-go");
    added(&mut ui, "Runbooks");
    let asked = serving.stand_in.asked();
    let key = format!("Bearer {KEY}");
    assert!(asked.iter().all(|asked| asked.headers["authorization"] == key.as_str()), "each with the key");
}

/// A key that's kept, then a server that has none. Its name alone is saved without asking it anything. One saved anyway
/// has its tools listed again, where the agent can, and the panel says so once that's on offer.
#[gpui_kit::test]
fn an_edit_is_checked_where_it_changes_what_the_server_is_sent_and_is_saved_anyway(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::new(), refusing());
    let mut ui = in_servers(cx, &stack);
    let runbooks = added_elsewhere(&stack, "runbooks", "Runbooks", &serving.url(), with_key());
    ui.wait("its row", |ui| ui.has(more(&runbooks)));

    edit(&mut ui, &runbooks);
    type_in(&mut ui, "mcp-name", "Northwind");
    ui.press("enter");
    added(&mut ui, "Northwind");
    assert!(serving.stand_in.asked().is_empty());

    edit(&mut ui, &runbooks);
    type_in(&mut ui, "mcp-header", "X-Key");
    assert_eq!(says(&mut ui), "Running agents have the change at once.");
    ui.press("enter");
    assert_eq!(refused(&mut ui), "Northwind refused this key.");
    assert_eq!(buttons(&mut ui), ("Try again".to_string(), Some("Save anyway".to_string())));
    assert_eq!(says(&mut ui), format!("Running agents have the change at once. {TOOLS}"));
    assert_eq!(serving.stand_in.asked()[0].headers["x-key"], KEY);
    let header = |ui: &Ui<'_>| match named(ui, "Northwind").unwrap().auth {
        ToolServerAuth::Key { header } => header.as_str().to_owned(),
        other => panic!("{other:?}"),
    };
    assert_eq!(header(&ui), "Authorization", "nothing changes");
    ui.click("mcp-anyway");
    ui.wait("its header", |ui| header(ui) == "X-Key" && !ui.has("mcp-panel"));

    edit(&mut ui, &runbooks);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), "Northwind asks who you are, so it needs a sign-in or a key.");
    serving.stand_in.change(|script| script.asks = ToolServerAsks::Nothing);
    ui.click("mcp-panel-go");
    ui.wait("neither", |ui| named(ui, "Northwind").unwrap().auth == ToolServerAuth::Neither && !ui.has("mcp-panel"));
}

/// The server has gone meanwhile from where it was, and from the hub.
#[gpui_kit::test]
fn a_server_that_cant_be_reached_says_so_and_one_the_hub_wont_have_is_refused_as_it_would_be(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_servers(cx, &stack);
    let nobody = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap();
    fill_in(&mut ui, "Docs", &format!("http://{nobody}/mcp"));
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), format!("The hub can't reach {nobody}. Connection refused."));

    added_elsewhere(&stack, "docs", "Docs", &open_to_anyone().url(), ToolServerAuthGiven::Neither);
    ui.click("mcp-anyway");

    let taken = "Couldn't add it: another MCP server is called Docs";
    ui.wait("the hub's refusal", |ui| !ui.has("mcp-anyway"));
    assert_eq!(refused(&mut ui), taken);
    assert_eq!(buttons(&mut ui), ("Add".to_string(), None));
}

/// The window on a fake hub that holds `world`, with Add's panel filled in, with a key.
pub(super) fn filled_in_on(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("Add server to be on", |ui| ui.on("mcp-add"));
    fill_in(&mut ui, "Runbooks", "https://mcp.example.com/mcp");
    choose(&mut ui, KEY_WAY);
    ui.input(KEY);
    (fake, ui)
}

#[gpui_kit::test]
fn a_hub_from_before_checks_is_asked_to_add_it_at_once(cx: &mut TestAppContext) {
    let mut world = World::default();
    let refuses = ApiError::new(ErrorCode::Conflict, "another MCP server is called Runbooks");
    world.tool_server_add_refused = Some(refuses);
    let asked = world.asked.clone();
    let (_fake, mut ui) = filled_in_on(cx, world);

    ui.click("mcp-panel-go");

    assert_eq!(refused(&mut ui), "Couldn't add it: another MCP server is called Runbooks");
    assert_eq!(buttons(&mut ui), ("Add".to_string(), None));
    assert_eq!((asked.count("tool_server_check"), asked.count("tool_server_add")), (1, 1));
}

#[gpui_kit::test]
fn a_check_the_hub_refuses_adds_nothing(cx: &mut TestAppContext) {
    let mut world = World::default();
    let refuses = ApiError::new(ErrorCode::Unavailable, "too many are being asked");
    world.tool_server_check = ToolServerCheckWorld::Answers(Err(refuses));
    let asked = world.asked.clone();
    let (_fake, mut ui) = filled_in_on(cx, world);

    ui.click("mcp-panel-go");

    assert_eq!(refused(&mut ui), "Couldn't add it: too many are being asked");
    assert_eq!(buttons(&mut ui), ("Add".to_string(), None));
    assert_eq!((asked.count("tool_server_check"), asked.count("tool_server_add")), (1, 0));
}

/// So that running agents are asked to connect to it again. The fake hub saves neither.
#[gpui_kit::test]
fn an_edit_thats_saved_anyway_says_so_to_the_hub(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.tool_servers = vec![tool_server("runbooks")];
    let refused_key = ToolServerChecked::NotListed { why: NoToolList::KeyRefused { status: 401 } };
    world.tool_server_check = ToolServerCheckWorld::Answers(Ok(refused_key));
    let asked = world.asked.clone();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("its row", |ui| ui.has("mcp-more-runbooks"));
    let saved_anyway = || {
        let edit = asked.last("tool_server_edit").unwrap();
        (edit["id"].as_str().unwrap().to_owned(), edit["saved_anyway"].as_bool())
    };

    edit(&mut ui, &"runbooks".into());
    type_in(&mut ui, "mcp-name", "Northwind");
    ui.click("mcp-panel-go");
    refused(&mut ui);
    assert_eq!(saved_anyway(), ("runbooks".into(), None), "where the server isn't asked");
    type_in(&mut ui, "mcp-key", KEY);
    ui.click("mcp-panel-go");
    assert_eq!(refused(&mut ui), "Northwind refused this key.");
    ui.click("mcp-anyway");

    ui.wait("the hub's refusal", |ui| !ui.has("mcp-anyway"));
    assert_eq!(saved_anyway(), ("runbooks".into(), Some(true)));
}

/// Under the last row of more servers than fit, where the panel's end is at the page's end as it opens.
#[gpui_kit::test]
fn a_panel_thats_longer_with_the_reason_why_has_its_end_in_sight(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.tool_servers = (1..=16).map(|at| tool_server(&format!("server-{at:02}"))).collect();
    let refused_key = ToolServerChecked::NotListed { why: NoToolList::KeyRefused { status: 401 } };
    world.tool_server_check = ToolServerCheckWorld::Answers(Ok(refused_key));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("their rows", |ui| ui.has("mcp-more-server-16"));
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    let ends = |ui: &mut Ui<'_>| (ui.bounds("mcp-panel").bottom(), ui.bounds("settings-body").bottom());
    edit(&mut ui, &"server-16".into());
    ui.bring_the_next_frame();
    let (panel, page) = ends(&mut ui);
    assert!(panel <= page, "as it opens: {panel:?}, {page:?}");
    type_in(&mut ui, "mcp-key", KEY);

    ui.click("mcp-panel-go");
    ui.wait("the hub's answer", |ui| ui.has("mcp-anyway"));
    assert_eq!(buttons(&mut ui), ("Try again".to_string(), Some("Save anyway".to_string())));
    ui.bring_the_next_frame();

    let (panel, page) = ends(&mut ui);
    assert!(panel <= page, "with the reason: {panel:?}, {page:?}");
}

/// One that's signed in to, whose query is changed, and one whose name is. The fake hub saves neither.
#[gpui_kit::test]
fn an_edit_that_asks_nothing_of_its_server_is_saved_at_once(cx: &mut TestAppContext) {
    let mut world = World::default();
    let signed_in = ToolServer { name: ToolServerName::stored("Linear".into()), auth: WORKS, ..tool_server("linear") };
    let keyed = ToolServer { name: ToolServerName::stored("Runbooks".into()), ..tool_server("runbooks") };
    world.tool_servers = vec![signed_in, keyed];
    let asked = world.asked.clone();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    ui.wait("their rows", |ui| ui.has("mcp-more-runbooks"));

    for (server, field, typed) in [("linear", "mcp-url", "?tools=a"), ("runbooks", "mcp-name", " two")] {
        edit(&mut ui, &server.into());
        ui.click(field);
        ui.press("end");
        ui.input(typed);
        ui.click("mcp-panel-go");
        assert_eq!(refused(&mut ui), "Couldn't save it: this hub can't do that until it's updated", "{server}");
        ui.click("mcp-cancel");
    }

    assert_eq!((asked.count("tool_server_check"), asked.count("tool_server_edit")), (0, 2));
}
