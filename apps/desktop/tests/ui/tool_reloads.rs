//! Reload on the plug's card: which rows have it, and what a press does, by the pointer and by the keyboard. Which rows
//! offer it, as their agent's run claims, is `client-core`'s rule, which its own tests pin.

#![cfg(test)]

use client_core::fake_hub::{Asked, World};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::{observed, tool_server};
use protocol::model::{AgentKind, RunCapabilities, RunId, ToolServerAuthGiven};

use desktop::workspace_pane::WorkspacePane;

use crate::support::{FakeHub, Stack, Ui, open_on_the_chat};
use crate::tool_servers::{chat, named, open_card, plug_says, said};

/// More than there are stops for Tab in a chat's window before its plug.
const MOST_TABS: usize = 60;

/// A chat of `agent`'s, whose run says it has two of the hub's servers, one that works and one it can't reach, and one of
/// its CLI's own, which it must be signed in to from the CLI.
fn with_servers(agent: AgentKind) -> World {
    let (mut world, chat) = chat();
    world.tool_servers = ["Notion", "Runbooks"].map(|name| named(name, tool_server(&name.to_lowercase()))).into();
    let notion = ToolServerState { tools: Some(14), ..said("Notion", Some("notion"), ServerState::Connected) };
    let runbooks = ToolServerState {
        error: Some("Connection refused".into()),
        ..said("Runbooks", Some("runbooks"), ServerState::Failed)
    };
    let docs = said("docs-search", None, ServerState::NeedsSignIn);
    world.observe(observed::tool_servers(chat, vec![notion, runbooks, docs]));
    let description = agents::descriptions().into_iter().find(|description| description.kind == agent).unwrap();
    let session = world.sessions.last_mut().unwrap();
    session.agent = agent;
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

fn offered(cx: &mut TestAppContext, agent: AgentKind) -> Vec<ElementId> {
    let (_fake, mut ui) = open(cx, with_servers(agent));
    open_card(&mut ui);
    ui.shown("mcp-reload-")
}

#[gpui_kit::test]
fn a_claude_chats_rows_have_it_and_a_codex_chats_where_the_server_doesnt_work(cx: &mut TestAppContext) {
    let [notion, runbooks, docs] =
        ["Notion", "Runbooks", "docs-search"].map(|name| ElementId::from(format!("mcp-reload-{name}")));

    assert_eq!(offered(cx, AgentKind::Claude), [notion, runbooks.clone(), docs.clone()], "its CLI's own too");
    assert_eq!(offered(cx, AgentKind::Codex), [runbooks, docs]);
    assert_eq!(offered(cx, AgentKind::Antigravity), []);
}

/// Whether the agent is connected to it as it was, or couldn't reach it.
#[gpui_kit::test]
fn one_to_be_connected_to_once_the_turn_ends_says_so_under_its_reload(cx: &mut TestAppContext) {
    let mut world = with_servers(AgentKind::Claude);
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
    let mut world = with_servers(AgentKind::Claude);
    world.observations.clear();
    let (_fake, mut ui) = open(cx, world);

    open_card(&mut ui);

    assert!(ui.shows_label("mcp-card", "Notion") && ui.shown("mcp-reload-").is_empty());
}

/// The pointer rests on the plug, crosses to a row's button, and presses it. It's off from then on, since the hub doesn't
/// say what comes of it.
#[gpui_kit::test]
fn a_press_asks_the_hub_once_and_the_button_is_off_until_the_agent_says_more(cx: &mut TestAppContext) {
    let world = with_servers(AgentKind::Claude);
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

    assert!(ui.off("mcp-reload-Runbooks"), "with no wait for the hub");
    ui.wait("the hub to be asked", |_| asked.count("session_reload_tool_server") == 1);
    ui.wait_for_every_call();
    assert!(ui.off("mcp-reload-Runbooks"), "though it has answered");
    assert!(ui.on("mcp-reload-Notion") && ui.on("mcp-reload-docs-search"), "the others are theirs");
    assert!(!ui.visible("mcp-reload-Notion"), "until its row is pointed at");
    ui.hover("mcp-hubs-0");
    ui.click("mcp-reload-Notion");
    ui.wait("the other too", |_| asked.count("session_reload_tool_server") == 2);
    assert!(plug_says(&mut ui, "MCP servers"), "as quiet as before");
}

/// On a real hub, with nothing listening where the server is. The button is off from the press until the agent has said
/// more of it, so it's on again only once the agent has tried.
#[gpui_kit::test]
fn a_running_agent_tries_the_server_again_and_its_row_says_what_came_of_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    client.tool_server_add(&"t".into(), "Docs", "http://127.0.0.1:9/mcp", ToolServerAuthGiven::Neither).unwrap();
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers"));
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    // The ring and the bars come between the plug and Send, so it moves from under the pointer.
    ui.wait("the ring and the bars", |ui| ui.has("context") && ui.has("usage"));
    open_card(&mut ui);
    ui.wait("what the agent says", |ui| ui.on("mcp-reload-Docs"));

    ui.click("mcp-reload-Docs");

    ui.wait("the agent to have tried it again", |ui| ui.on("mcp-reload-Docs"));
    assert!(ui.shows_label("mcp-card", "Docs, Can't be reached"));
}

#[gpui_kit::test]
fn what_the_hub_refuses_is_said_under_the_row_and_its_button_can_be_pressed_again(cx: &mut TestAppContext) {
    let mut world = with_servers(AgentKind::Claude);
    world.tool_server_reload_refused = Some(ApiError::new(ErrorCode::Conflict, "the chat's agent isn't running"));
    let (_fake, mut ui) = open(cx, world);
    open_card(&mut ui);
    ui.hover("mcp-hubs-0");

    ui.click("mcp-reload-Notion");

    let refused = "Notion, 14 tools, Couldn't reload it: the chat's agent isn't running";
    ui.wait("why", |ui| ui.shows_label("mcp-card", refused));
    assert!(ui.on("mcp-reload-Notion"));
    ui.click("mcp-reload-Runbooks");
    let refused = "Runbooks, Can't be reached, Couldn't reload it: the chat's agent isn't running";
    ui.wait("why", |ui| ui.shows_label("mcp-card", refused));
}

#[gpui_kit::test]
fn a_hub_that_doesnt_know_it_is_asked_once_and_then_no_row_has_it_until_the_next_connection(cx: &mut TestAppContext) {
    let mut world = with_servers(AgentKind::Claude);
    world.tool_server_reload_refused = Some(ApiError::new(ErrorCode::Unsupported, "unknown request"));
    let (_fake, mut ui) = open(cx, world);
    open_card(&mut ui);

    ui.click("mcp-reload-Runbooks");

    ui.wait("none", |ui| ui.shown("mcp-reload-").is_empty());
    let why = "Runbooks, Can't be reached, Couldn't reload it: this hub can't do that until it's updated";
    assert!(ui.shows_label("mcp-card", why));
    let (pane, client) = (ui.pane().unwrap(), ui.view().client().unwrap().clone());
    ui.cx.update(|cx| pane.update(cx, |pane, cx| pane.reconnect(&client, cx)));
    assert_eq!(ui.shown("mcp-reload-").len(), 3, "which can be another build's");
}

#[gpui_kit::test]
fn every_one_is_off_while_the_hub_is_away(cx: &mut TestAppContext) {
    let (_fake, mut ui) = open(cx, with_servers(AgentKind::Claude));
    open_card(&mut ui);
    assert!(ui.on("mcp-reload-Notion") && ui.on("mcp-reload-Runbooks"));

    let pane = ui.pane().unwrap();
    ui.cx.update(|cx| pane.update(cx, WorkspacePane::hub_went_away));

    assert!(ui.off("mcp-reload-Notion") && ui.off("mcp-reload-Runbooks"));
}

#[gpui_kit::test]
fn a_press_on_the_plug_or_its_card_leaves_the_focus_in_the_composer(cx: &mut TestAppContext) {
    let (_fake, mut ui) = open(cx, with_servers(AgentKind::Claude));
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
    let world = with_servers(AgentKind::Claude);
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
    assert!(ui.off("mcp-reload-Notion") && focused(&mut ui, "mcp-reload-Notion"), "it keeps the focus");
    ui.press("shift-tab");
    assert!(ui.has("mcp-card"), "the plug");
    ui.press("shift-tab");

    ui.wait_out_a_tooltip();
    assert!(!ui.has("mcp-card"), "what's before the plug");
    ui.press("tab");
    assert!(ui.has("mcp-card"), "the plug again");
    ui.press("tab");
    ui.press("tab");
    assert!(focused(&mut ui, "mcp-reload-docs-search"), "past the one that's off");
    ui.press("tab");
    ui.wait_out_a_tooltip();
    assert!(!ui.has("mcp-card"), "what's after the plug");
}
