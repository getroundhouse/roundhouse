//! Reload on the plug's card: what a press does, by the pointer and by the keyboard. Which rows offer it, as their agent's
//! run claims, is `client-core`'s rule, which its own tests pin, and what comes of it is the hub's and the agent's, which
//! the daemon's tests pin.

#![cfg(test)]

use client_core::fake_hub::{Asked, World};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::{observed, tool_server};
use protocol::model::{AgentKind, RunCapabilities, RunId};

use desktop::workspace_pane::WorkspacePane;

use crate::support::{FakeHub, Ui, open_on_the_chat};
use crate::tool_servers::{chat, named, open_card, plug_says, said};

/// More than there are stops for Tab in a chat's window before its plug.
const MOST_TABS: usize = 60;

/// A Claude chat, whose run says it has two of the hub's servers, one that works and one it can't reach, and one of its
/// CLI's own, which it must be signed in to from the CLI.
fn with_servers() -> World {
    let (mut world, chat) = chat();
    world.tool_servers = ["Notion", "Runbooks"].map(|name| named(name, tool_server(&name.to_lowercase()))).into();
    let notion = ToolServerState { tools: Some(14), ..said("Notion", Some("notion"), ServerState::Connected) };
    let runbooks = ToolServerState {
        error: Some("Connection refused".into()),
        ..said("Runbooks", Some("runbooks"), ServerState::Failed)
    };
    let docs = said("docs-search", None, ServerState::NeedsSignIn);
    world.observe(observed::tool_servers(chat, vec![notion, runbooks, docs]));
    let description =
        agents::descriptions().into_iter().find(|description| description.kind == AgentKind::Claude).unwrap();
    let session = world.sessions.last_mut().unwrap();
    session.agent = AgentKind::Claude;
    session.agent_capabilities = Some(RunCapabilities::new(RunId::new(1), description.capabilities));
    world
}

/// The window on `world`, with its one chat's plug drawn.
fn open(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let chat = world.sessions.last().unwrap().id.clone();
    let (fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers"));
    (fake, ui)
}

/// Whether the agent is connected to it as it was, or couldn't reach it.
#[gpui_kit::test]
fn one_to_be_connected_to_once_the_turn_ends_says_so_under_its_reload(cx: &mut TestAppContext) {
    let mut world = with_servers();
    let chat = world.sessions.last().unwrap().id.clone();
    let waits = |name: &str, state| ToolServerState {
        error: Some("Connection refused".into()),
        connects_again: Some(ConnectsAgain::WhenItsTurnEnds),
        ..said(name, Some(&name.to_lowercase()), state)
    };
    let servers = vec![waits("Notion", ServerState::Connected), waits("Runbooks", ServerState::Failed)];
    world.observations.clear();
    world.observe(observed::tool_servers(chat, servers));
    let asked: Asked = world.asked.clone();
    let (_fake, mut ui) = open(cx, world);

    open_card(&mut ui);

    for server in ["Notion", "Runbooks"] {
        assert!(ui.shows_label("mcp-card", &format!("{server}, When its turn ends")), "{server}");
    }
    assert!(ui.visible("mcp-reload-Notion") && ui.visible("mcp-reload-Runbooks"), "with no row pointed at");
    ui.click("mcp-reload-Notion");
    ui.wait("the hub to be asked", |_| asked.count("session_reload_tool_server") == 1);
}

/// An observation is forgotten as its run ends, so it's the hub's record of what the run claims that's left.
#[gpui_kit::test]
fn a_chat_whose_agent_isnt_running_has_none(cx: &mut TestAppContext) {
    let mut world = with_servers();
    world.observations.clear();
    let (_fake, mut ui) = open(cx, world);

    open_card(&mut ui);

    assert!(ui.shows_label("mcp-card", "Notion") && ui.shown("mcp-reload-").is_empty());
}

/// The pointer rests on the plug, crosses to a row's button, and presses it. The row says what comes of it as the agent
/// says, so nothing changes here until it does.
#[gpui_kit::test]
fn a_press_asks_the_hub_to_have_the_agent_connect_to_that_server_again(cx: &mut TestAppContext) {
    let world = with_servers();
    let asked: Asked = world.asked.clone();
    let (_fake, mut ui) = open(cx, world);
    open_card(&mut ui);
    for server in
        ["Notion, 14 tools", "Runbooks, Can't be reached, Connection refused", "docs-search, Sign in from its CLI"]
    {
        assert!(ui.shows_label("mcp-card", server), "{server}");
    }
    assert!(ui.shows_label("mcp-card", "Reload Notion") && ui.shows_label("mcp-card", "Reload Runbooks"));

    ui.hover("new-tab");
    assert!(ui.has("mcp-card"), "the pointer can be on its way to a button");
    ui.hover("mcp-reload-Runbooks");
    ui.wait_out_a_tooltip();
    ui.click("mcp-reload-Runbooks");

    ui.wait("the hub to be asked", |_| asked.count("session_reload_tool_server") == 1);
    let reload = asked.last("session_reload_tool_server").unwrap();
    assert_eq!(reload["server"], "Runbooks");
    assert!(!ui.visible("mcp-reload-Notion"), "until its row is pointed at");
    ui.hover("mcp-hubs-0");
    ui.click("mcp-reload-Notion");
    ui.wait("the other too", |_| asked.count("session_reload_tool_server") == 2);
    assert!(plug_says(&mut ui, "MCP servers"), "as quiet as before");
}

/// It's said along the window's top, as the pane's other refusals are, and the card stays open over the composer.
#[gpui_kit::test]
fn what_the_hub_refuses_is_said_as_the_panes_other_refusals_are(cx: &mut TestAppContext) {
    let mut world = with_servers();
    world.tool_server_reload_refused = Some(ApiError::new(ErrorCode::Conflict, "the chat's agent isn't running"));
    let (_fake, mut ui) = open(cx, world);
    open_card(&mut ui);

    ui.click("mcp-reload-Runbooks");

    let refused = "Couldn't reload it: the chat's agent isn't running";
    ui.wait("why", |ui| ui.view().error.as_deref() == Some(refused));
    assert!(ui.visible("mcp-card") && ui.on("mcp-reload-Runbooks"), "it can be pressed again");
    assert!(ui.shows_label("mcp-card", "Runbooks, Can't be reached, Connection refused"), "the row says nothing of it");
}

#[gpui_kit::test]
fn a_hub_that_doesnt_know_it_is_asked_once_and_then_no_row_has_it_until_the_next_connection(cx: &mut TestAppContext) {
    let mut world = with_servers();
    world.tool_server_reload_refused = Some(ApiError::new(ErrorCode::Unsupported, "unknown request"));
    let (_fake, mut ui) = open(cx, world);
    open_card(&mut ui);

    ui.click("mcp-reload-Runbooks");

    ui.wait("none", |ui| ui.shown("mcp-reload-").is_empty());
    let why = "Couldn't reload it: this hub can't do that until it's updated";
    assert_eq!(ui.view().error.as_deref(), Some(why));
    crate::catching_up::connect_again(&mut ui);
    ui.wait("each, on the next connection, which can be another build's", |ui| ui.shown("mcp-reload-").len() == 3);
}

#[gpui_kit::test]
fn every_one_is_off_while_the_hub_is_away(cx: &mut TestAppContext) {
    let (_fake, mut ui) = open(cx, with_servers());
    open_card(&mut ui);
    assert!(ui.on("mcp-reload-Notion") && ui.on("mcp-reload-Runbooks"));

    let pane = ui.pane().unwrap();
    ui.cx.update(|cx| pane.update(cx, WorkspacePane::hub_went_away));

    assert!(ui.off("mcp-reload-Notion") && ui.off("mcp-reload-Runbooks"));
}

#[gpui_kit::test]
fn a_press_on_the_plug_or_its_card_leaves_the_focus_in_the_composer(cx: &mut TestAppContext) {
    let (_fake, mut ui) = open(cx, with_servers());
    ui.press("a");
    open_card(&mut ui);

    ui.click("mcp");
    ui.press("b");
    ui.click("mcp-hubs-0");
    ui.press("c");

    assert_eq!(ui.composer_text(), "abc");
}

/// Whether the button with `id` has the keyboard's focus.
fn focused(ui: &mut Ui<'_>, id: &'static str) -> bool {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.try_find(id).and_then(|e| e.focused())).unwrap() == Some(true)
}

/// One walk, with the pointer nowhere near: Tab comes to the plug, which shows its card, then to each row's button, where
/// Enter presses it, and on past the last to what's after the plug. Shift-Tab comes back.
#[gpui_kit::test]
fn the_keyboard_reaches_every_button_of_the_card_through_the_plug(cx: &mut TestAppContext) {
    let world = with_servers();
    let asked: Asked = world.asked.clone();
    let (_fake, mut ui) = open(cx, world);
    ui.bring_to_front();
    let tabs = (1..=MOST_TABS).find(|_| {
        ui.press("tab");
        ui.has("mcp-card")
    });
    assert!(tabs.is_some(), "no stop of Tab's showed the card");
    for button in ["mcp-reload-Notion", "mcp-reload-Runbooks", "mcp-reload-docs-search"] {
        ui.press("tab");
        assert!(focused(&mut ui, button) && ui.visible(button), "{button}");
    }
    ui.press("shift-tab");
    ui.press("shift-tab");
    assert!(focused(&mut ui, "mcp-reload-Notion"));
    ui.wait_out_a_tooltip();
    assert!(ui.has("mcp-card"), "for as long as the focus is in it");

    ui.press("enter");
    ui.wait("the hub to be asked", |_| asked.count("session_reload_tool_server") == 1);
    assert!(focused(&mut ui, "mcp-reload-Notion"), "it keeps the focus");
    ui.press("shift-tab");
    assert!(ui.has("mcp-card"), "the plug");
    ui.press("shift-tab");

    ui.wait_out_a_tooltip();
    assert!(!ui.has("mcp-card"), "what's before the plug");
    ui.press("tab");
    assert!(ui.has("mcp-card"), "the plug again");
    for _ in 0..3 {
        ui.press("tab");
    }
    assert!(focused(&mut ui, "mcp-reload-docs-search"));
    ui.press("tab");
    ui.wait_out_a_tooltip();
    assert!(!ui.has("mcp-card"), "what's after the plug");
}
