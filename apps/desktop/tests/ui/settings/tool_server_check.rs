//! A server asked for its tools as it's added or edited in Settings: what the panel says and does while it's asked and
//! once it hasn't listed them, and where its key is. Against a real hub and a stand-in for the
//! server, and against a fake hub in `tool_server_saves`. Every server and key is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::TestAppContext;
use hub::{ToolServerAsks, ToolServerGate, ToolServerScript};
use protocol::model::{ToolServerAuth, ToolServerAuthGiven};

use super::git_app::label_of;
use super::open_settings;
use super::tool_server_edit::edit;
use super::tool_servers::{
    KEY, KEY_WAY, NONE_WAY, added_elsewhere, choose, fill_in, in_servers, more, named, open_to_anyone, the_hubs,
    type_in, with_key,
};
use super::tool_sign_in::refused;
use crate::support::{Serving, Stack, Ui, files_holding};

const REFUSED_KEY: &str = "Runbooks refused this key.";

/// What its main button says, and whether there's one beside it that goes on anyway.
pub(super) fn buttons(ui: &mut Ui<'_>) -> (String, Option<String>) {
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

/// A server anyone can use, which holds its answers back until `gate` opens.
fn held_by(gate: &ToolServerGate) -> Serving {
    Serving::with(ToolServerScript {
        asks: ToolServerAsks::Nothing,
        server_after: Some(gate.clone()),
        ..ToolServerScript::default()
    })
}

/// Cancelled, then as Settings closes, and the last time typed in while it's asked, which changes nothing of what's
/// then added anyway.
#[gpui_kit::test]
fn a_server_is_asked_for_its_tools_before_its_added_with_its_panel_off_and_nothing_is_added_once_thats_let_go_of(
    cx: &mut TestAppContext,
) {
    let gate = ToolServerGate::closed();
    let (stack, serving) = (Stack::new(), held_by(&gate));
    let mut ui = in_servers(cx, &stack);
    let url = serving.url();
    let host = url.strip_prefix("http://").unwrap().split('/').next().unwrap();
    let says = format!("Asking {host} for its tools");
    let asked = |ui: &mut Ui<'_>| {
        assert_eq!(label_of(ui, "mcp-asking"), says);
        assert!(ui.off("mcp-panel-go") && ui.off("mcp-way"));
        assert_eq!(buttons(ui), ("Add".to_string(), None));
    };

    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    asked(&mut ui);
    ui.click("mcp-cancel");
    assert!(!ui.has("mcp-panel") && ui.on("mcp-add"));

    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-panel-go");
    asked(&mut ui);
    ui.click("settings-back");
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    assert!(!ui.has("mcp-panel"));
    gate.open();
    ui.wait_for_every_call();
    assert_eq!(the_hubs(&stack), Vec::<String>::new(), "nothing is added once it's let go of");

    let gate = ToolServerGate::closed();
    serving.stand_in.change(|script| {
        script.server_after = Some(gate.clone());
        script.asks = ToolServerAsks::Answers(401);
    });
    fill_in(&mut ui, "Docs", &url);
    choose(&mut ui, NONE_WAY);
    ui.click("mcp-name");
    ui.press("end");
    ui.press("enter");
    asked(&mut ui);
    ui.input(" Two");
    gate.open();
    assert_eq!(refused(&mut ui), "Docs asks who you are, so it needs a sign-in or a key.");
    ui.click("mcp-anyway");
    added(&mut ui, "Docs");
    assert_eq!(the_hubs(&stack), ["Docs"], "as it was typed before it was asked");
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

/// A key that's kept, then a server that has none. Its name alone is saved without asking it anything, and a header the
/// server refuses is saved anyway.
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
    ui.press("enter");
    assert_eq!(refused(&mut ui), "Northwind refused this key.");
    assert_eq!(buttons(&mut ui), ("Try again".to_string(), Some("Save anyway".to_string())));
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
