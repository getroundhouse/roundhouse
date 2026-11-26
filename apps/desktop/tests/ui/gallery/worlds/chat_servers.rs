//! The MCP servers a chat has, the hub's and the ones its agent's CLI has of its own, all made up.

#![cfg(test)]

use client_core::fake_hub::World;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::event::EventBody;
use protocol::fixtures::{observed, tool_server, tool_server_needing_sign_in};
use protocol::model::{AgentKind, ModelName, SessionStatus, ToolServer, ToolServerName};

use super::super::codex_screens::models;
use super::scenarios::{chat_world, context};
use super::tool_servers::page;

/// The conversation, as a Codex chat whose agent was launched before one of the hub's servers was added, and says why
/// it can't reach another.
pub(in super::super) fn chat_servers() -> World {
    let linear = named("Linear", tool_server("linear"));
    let runbooks = ToolServerState {
        error: Some("Connection refused".into()),
        ..said("Runbooks", Some("runbooks"), ServerState::Failed)
    };
    let mut world = with_servers(linear, vec![runbooks, with_tools("Notion", Some("notion"), 14)]);
    world.agents = Some(agents::descriptions());
    world.offer_models(AgentKind::Codex, &models());
    let chat = world.sessions.last_mut().unwrap();
    chat.agent = AgentKind::Codex;
    chat.model = ModelName::named("gpt-6.1-sol");
    world
}

/// A Claude chat, where one of the hub's needs you to sign in to it, which the hub answers with a page, and the agent
/// doesn't say why it can't reach another.
pub(in super::super) fn chat_servers_to_sign_in_to() -> World {
    let linear = named("Linear", tool_server_needing_sign_in("linear"));
    let said_of = vec![
        said("Linear", Some("linear"), ServerState::Failed),
        with_tools("Notion", Some("notion"), 14),
        said("Runbooks", Some("runbooks"), ServerState::Failed),
    ];
    let mut world = with_servers(linear, said_of);
    world.tool_sign_in_answer = Some(Ok(page()));
    world
}

/// A chat of `events`, in `status`, on a hub that has a server the person called `GitHub (Northwind)`.
pub(in super::super) fn chat_with_a_server(status: SessionStatus, events: Vec<EventBody>) -> World {
    let mut world = chat_world(status, events);
    world.tool_servers.push(named("GitHub (Northwind)", tool_server("github")));
    world
}

/// The context's chat, on a hub with `linear` and two more, whose agent says it has `said_of` and two of its CLI's own.
fn with_servers(linear: ToolServer, mut said_of: Vec<ToolServerState>) -> World {
    let mut world = context();
    world.tool_servers =
        vec![linear, named("Notion", tool_server("notion")), named("Runbooks", tool_server("runbooks"))];
    said_of.extend([said("docs-search", None, ServerState::NeedsSignIn), with_tools("browser", None, 3)]);
    let chat = world.sessions.last().unwrap().id.clone();
    world.observe(observed::tool_servers(chat, said_of));
    world
}

fn named(name: &str, server: ToolServer) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..server }
}

fn said(name: &str, hubs: Option<&str>, state: ServerState) -> ToolServerState {
    ToolServerState { name: name.into(), hubs: hubs.map(Into::into), state, error: None, tools: None }
}

fn with_tools(name: &str, hubs: Option<&str>, tools: u32) -> ToolServerState {
    ToolServerState { tools: Some(tools), ..said(name, hubs, ServerState::Connected) }
}
