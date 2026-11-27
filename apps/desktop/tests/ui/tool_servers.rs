//! The plug beside Send, and its card of the MCP servers the chat has: the hub's and the ones its agent's CLI has of its
//! own.

#![cfg(test)]

use client_core::fake_hub::events::{first_turn, replied, task, text, user_prompt};
use client_core::fake_hub::{Asked, World};
use fetching::serving::NOWHERE;
use gpui_kit::{AppContext as _, ElementId, ScrollDelta, TestAppContext, point, px};
use hub::{ToolServerPerson, ToolServerRefreshing};
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInPage};
use protocol::event::{EventBody, TaskKind, TaskTool};
use protocol::fixtures::{observed, tool_server, tool_server_needing_sign_in, tool_server_wanting_more_access};
use protocol::model::{SessionId, SessionStatus, ToolServer, ToolServerAuthGiven, ToolServerName};

use desktop::workspace_pane::WorkspacePane;

use crate::settings::open_settings;
use crate::support::{Serving, Stack, Ui, one_chat, open_on_the_chat, renewing_often};

/// Whether the plug is there, saying `said` to assistive apps, which is as much as its colour says.
pub(crate) fn plug_says(ui: &mut Ui<'_>, said: &str) -> bool {
    ui.labelled(said).iter().any(|plug| plug.path().last().is_some_and(|id| *id == "mcp".into()))
}

/// What the transcript calls the server its agent calls `server`.
fn called(ui: &Ui<'_>, server: &str) -> String {
    let transcript = ui.transcript().unwrap();
    ui.cx.read_entity(&transcript, |transcript, _| transcript.server_shown(&server.into()))
}

/// What the hub has and what the agent says of it both reach the card, with nothing asked: a server added while the
/// agent runs is from its next start, and one that's renamed is still the one the agent has, which its calls show too.
#[gpui_kit::test]
fn the_card_follows_the_hubs_servers_and_what_the_agent_says_of_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    assert!(!ui.has("mcp"), "a chat with no servers has no plug");
    // Nothing listens there, so the agent can't reach it.
    let add = |id: &str, name: &str| {
        client.tool_server_add(&id.into(), name, &format!("{NOWHERE}/mcp"), ToolServerAuthGiven::Neither).unwrap();
    };

    add("t-1", "Linear (Northwind)");
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers"));
    ui.hover("mcp");
    ui.wait_out_a_tooltip();
    assert!(ui.shows_label("mcp-card", "Linear (Northwind)"), "nobody has said anything of it");
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    // The ring and the bars come between the plug and Send, so it moves from under the pointer.
    ui.wait("the ring and the bars", |ui| ui.has("context") && ui.has("usage"));
    ui.hover("mcp");
    ui.wait_out_a_tooltip();
    ui.wait("what the agent says", |ui| ui.shows_label("mcp-card", "Linear (Northwind), Can't be reached"));
    assert_eq!(called(&ui, "Linear_Northwind"), "Linear (Northwind)");

    add("t-2", "Docs");
    let renamed =
        ToolServerEdit { id: "t-1".into(), name: Some("Linear".into()), url: None, auth: None, saved_anyway: false };
    client.tool_server_edit(renamed).unwrap();

    ui.wait("the server added", |ui| ui.shows_label("mcp-card", "Docs, From its next start"));
    ui.wait("the server renamed", |ui| ui.shows_label("mcp-card", "Linear, Can't be reached"));
    ui.wait("the chat's calls to it", |ui| called(ui, "Linear_Northwind") == "Linear");
    assert_eq!(called(&ui, "Linear"), "Linear", "as agents are given it from now on");
}

pub(crate) fn named(name: &str, server: ToolServer) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..server }
}

pub(crate) fn said(name: &str, hubs: Option<&str>, state: ServerState) -> ToolServerState {
    ToolServerState {
        name: name.into(),
        hubs: hubs.map(Into::into),
        state,
        error: None,
        tools: None,
        connects_again: None,
    }
}

/// A call to one of the hub's that went on in the background, as its agent lists it.
#[gpui_kit::test]
fn the_line_of_a_background_task_of_an_mcp_servers_tool_has_the_persons_name_for_the_server(cx: &mut TestAppContext) {
    let mut export = task(TaskKind::Command, "toolu-1", "Linear_Northwind/export_issues");
    export.tool = Some(TaskTool { server: "Linear_Northwind".into(), tool: "export_issues".into() });
    let listed = EventBody::AgentTasks { tasks: vec![export], stopped: Vec::new() };
    let said = [listed, text("msg-1", "hello"), replied("hello", 2, 0.01)];
    let (mut world, chat) = one_chat(SessionStatus::Background, first_turn(user_prompt("say:hello"), said));
    world.tool_servers.push(named("Linear (Northwind)", tool_server("t-1")));

    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);

    ui.wait("the task's line", |ui| !ui.labelled("Linear (Northwind): Export issues").is_empty());
}

/// A chat whose agent has answered once.
pub(crate) fn chat() -> (World, SessionId) {
    let events = first_turn(user_prompt("say:hello"), [text("msg-1", "hello"), replied("hello", 2, 0.01)]);
    one_chat(SessionStatus::Idle, events)
}

/// The card opens as the ring's does, once the pointer rests on the plug, and goes once it leaves.
#[gpui_kit::test]
fn the_plugs_card_shows_once_the_pointer_rests_and_the_plug_says_whats_to_be_signed_in_to(cx: &mut TestAppContext) {
    let (mut world, chat) = chat();
    world.tool_servers.push(named("Linear", tool_server_needing_sign_in("t-1")));
    world.tool_servers.push(named("Notion", tool_server("t-2")));
    let notion = ToolServerState { tools: Some(14), ..said("Notion", Some("t-2"), ServerState::Connected) };
    let failed =
        ToolServerState { error: Some("Connection refused".into()), ..said("docs", None, ServerState::Failed) };
    let sign_in = said("tickets", None, ServerState::NeedsSignIn);
    world.observe(observed::tool_servers(chat.clone(), vec![notion, failed, sign_in]));
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers, Linear needs you to sign in"));

    ui.hover("mcp");
    assert!(!ui.has("mcp-card"), "passing over the plug shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("mcp-card"), "resting on it does");

    for server in [
        "Linear, Needs you to sign in",
        "Sign in to Linear",
        "Notion, 14 tools",
        "docs, Can't be reached, Connection refused",
        "tickets, Sign in from its CLI",
    ] {
        assert!(ui.shows_label("mcp-card", server), "{server}");
    }
    let offered = ui.shown("mcp-sign-in-");
    assert_eq!(offered, [ElementId::from("mcp-sign-in-t-1")], "only one of the hub's is signed in to from here");
    ui.hover("new-tab");
    ui.wait_out_a_tooltip();
    ui.wait("the card to go", |ui| !ui.has("mcp-card"));
}

/// Rests the pointer on the plug until its card shows.
pub(crate) fn open_card(ui: &mut Ui<'_>) {
    ui.hover("mcp");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("mcp-card"));
}

/// Waits for the browser to be opened on a page, other than `before`.
fn page_opened(ui: &mut Ui<'_>, before: Option<&str>) -> String {
    ui.wait("the server's page", |ui| ui.cx.opened_url().as_deref() != before);
    ui.cx.opened_url().unwrap()
}

/// One walk: a sign-in started in one workspace's chat shows in another's, where it's refused and started again, and
/// the plug turns quiet once the hub's record says it works.
#[gpui_kit::test]
fn a_sign_in_from_the_card_is_every_chats_and_can_be_tried_again_until_the_hub_says_it_works(cx: &mut TestAppContext) {
    let (stack, serving) = (Stack::in_environment(&renewing_often()), Serving::with_short_tokens());
    let linear = serving.signed_in_elsewhere(&stack);
    serving.refuses_to_renew();
    let button = || ElementId::from(format!("mcp-sign-in-{linear}"));
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.wait("the plug to say so", |ui| plug_says(ui, "MCP servers, Linear needs you to sign in"));
    serving.stand_in.change(|script| {
        (script.refreshing, script.person) = (ToolServerRefreshing::Works, ToolServerPerson::Refuses);
    });
    open_card(&mut ui);

    ui.click(button());

    let page = page_opened(&mut ui, None);
    ui.wait("the row to say so", |ui| ui.shows_label("mcp-card", "Linear, Signing in"));
    assert!(!ui.has(button()), "there's nothing to press while it's under way");
    assert!(plug_says(&mut ui, "MCP servers, Linear needs you to sign in"), "as the hub's record does");
    ui.new_workspace();
    open_card(&mut ui);
    assert!(ui.shows_label("mcp-card", "Linear, Signing in"), "in another workspace's chat");
    serving.sent_back_to(&page).follow();
    let why = "Linear, Needs you to sign in, Couldn't sign in: the sign-in was refused";
    ui.wait("why it came to nothing", |ui| ui.shows_label("mcp-card", why));
    serving.stand_in.change(|script| script.person = ToolServerPerson::Approves);
    ui.click(button());
    let again = page_opened(&mut ui, Some(&page));
    assert!(ui.shows_label("mcp-card", "Linear, Signing in"));
    serving.sent_back_to(&again).follow();
    ui.wait("the plug to be quiet", |ui| plug_says(ui, "MCP servers"));
    ui.wait("the card to follow", |ui| ui.shows_label("mcp-card", "Linear"));
    assert!(!ui.has(button()));
}

/// A fake hub's chat, with two of the hub's servers to sign in to, and a hub that answers a sign-in with `answer`.
fn two_to_sign_in_to(answer: ToolSignInBegun) -> (World, SessionId) {
    let (mut world, chat) = chat();
    world.tool_servers =
        ["Linear", "Notion"].map(|name| named(name, tool_server_needing_sign_in(&name.to_lowercase()))).into();
    world.tool_sign_in_answer = Some(Ok(answer));
    (world, chat)
}

#[gpui_kit::test]
fn the_button_signs_in_to_its_own_server_and_starting_another_takes_its_place(cx: &mut TestAppContext) {
    let page = ToolSignInPage::new("https://auth.example.com/authorize?state=made-up");
    let (world, chat) = two_to_sign_in_to(ToolSignInBegun::Page { page });
    let asked: Asked = world.asked.clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers, Linear, Notion need you to sign in"));
    open_card(&mut ui);

    ui.click("mcp-sign-in-notion");

    assert_eq!(page_opened(&mut ui, None), "https://auth.example.com/authorize?state=made-up");
    assert!(ui.shows_label("mcp-card", "Notion, Signing in"));
    assert!(ui.on("mcp-sign-in-linear") && ui.shows_label("mcp-card", "Linear, Needs you to sign in"));
    ui.click("mcp-sign-in-linear");
    ui.wait("the other", |ui| ui.shows_label("mcp-card", "Linear, Signing in"));
    assert!(ui.on("mcp-sign-in-notion") && ui.shows_label("mcp-card", "Notion, Needs you to sign in"));
    // It's asked on a thread of its own, so the hub may not have it yet as the card says it's signing in.
    crate::support::wait_until("the hub to be asked to start the second", || asked.count("tool_sign_in_start") >= 2);
    assert_eq!(asked.count("tool_sign_in_start"), 2);
}

#[gpui_kit::test]
fn the_button_is_off_while_the_hub_is_away_and_says_why_where_theres_no_way_to_sign_in(cx: &mut TestAppContext) {
    let (world, chat) = two_to_sign_in_to(ToolSignInBegun::NoSignIn);
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers, Linear, Notion need you to sign in"));
    open_card(&mut ui);
    ui.click("mcp-sign-in-notion");
    let why = "Notion, Needs you to sign in, This server has no sign-in. Anyone who can reach it can use it.";
    ui.wait("why there's no way", |ui| ui.shows_label("mcp-card", why));
    assert!(ui.on("mcp-sign-in-notion") && ui.on("mcp-sign-in-linear"));

    let pane = ui.pane().unwrap();
    ui.cx.update(|cx| pane.update(cx, WorkspacePane::hub_went_away));

    assert!(ui.off("mcp-sign-in-notion") && ui.off("mcp-sign-in-linear"));
}

/// It works, so it's as one that needs signing in to is, but for saying what for.
#[gpui_kit::test]
fn a_server_that_wants_more_access_is_signed_in_to_again_from_the_card_or_from_settings(cx: &mut TestAppContext) {
    let (mut world, chat) = chat();
    let page = ToolSignInPage::new("https://auth.example.com/authorize?state=made-up");
    world.tool_servers.push(named("Linear", tool_server_wanting_more_access("linear")));
    world.tool_sign_in_answer = Some(Ok(ToolSignInBegun::Page { page }));
    let asked: Asked = world.asked.clone();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| plug_says(ui, "MCP servers, Linear wants more access"));
    open_card(&mut ui);
    assert!(ui.shows_label("mcp-card", "Linear, Needs you to sign in again, Wants more access"));

    ui.click("mcp-sign-in-linear");

    page_opened(&mut ui, None);
    assert!(ui.shows_label("mcp-card", "Linear, Signing in"));
    open_settings(&mut ui);
    ui.click("settings-page-mcp");
    assert!(ui.shows_label("mcp-server-linear", "Sign in for more access"));
    ui.wait("the sign-in from the card", |ui| ui.has("mcp-cancel"));
    ui.click("mcp-cancel");
    ui.click("mcp-more-linear");
    ui.click_menu_item("Sign in again");
    ui.wait("another", |ui| ui.has("mcp-open-page"));
    assert_eq!(asked.count("tool_sign_in_start"), 2);
}

#[gpui_kit::test]
fn a_card_of_more_servers_than_fit_stays_in_the_window_and_scrolls_to_the_last(cx: &mut TestAppContext) {
    let (mut world, chat) = chat();
    let server = |n: usize| named(&format!("Server {n:02}"), tool_server_needing_sign_in(&format!("t-{n:02}")));
    world.tool_servers = (0..40).map(server).collect();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("the plug", |ui| ui.has("mcp"));

    open_card(&mut ui);

    let window = ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size()).unwrap();
    let card = ui.bounds("mcp-card");
    assert!(card.top() >= px(0.) && card.bottom() <= window.height, "{card:?} in {window:?}");
    assert!(ui.visible("mcp-sign-in-t-00") && !ui.visible("mcp-sign-in-t-39"));
    ui.scroll("mcp-listed", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(ui.visible("mcp-sign-in-t-39") && !ui.visible("mcp-sign-in-t-00"));
}
