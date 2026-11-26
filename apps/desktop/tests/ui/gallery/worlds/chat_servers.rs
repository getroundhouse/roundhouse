//! The MCP servers a chat has, the hub's and the ones its agent's CLI has of its own, all made up.

#![cfg(test)]

use client_core::fake_hub::World;
use protocol::capability::setup::{ConnectsAgain, ServerState, ToolServerState};
use protocol::event::EventBody;
use protocol::fixtures::{observed, tool_server, tool_server_needing_sign_in, tool_server_wanting_more_access};
use protocol::model::{
    AgentKind, ModelName, RunCapabilities, RunId, SessionStatus, ToolServer, ToolServerName, ToolServerUrl,
};

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
    chat.agent_capabilities = running(AgentKind::Codex);
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

/// A Claude chat, where one of the hub's works, and wants more access than its sign-in has.
pub(in super::super) fn chat_servers_wanting_more_access() -> World {
    let linear = ToolServer {
        url: ToolServerUrl::stored("https://mcp.linear.app/mcp".into()),
        ..named("Linear", tool_server_wanting_more_access("linear"))
    };
    let said_of = vec![
        with_tools("Linear", Some("linear"), 23),
        with_tools("Notion", Some("notion"), 14),
        with_tools("Runbooks", Some("runbooks"), 2),
    ];
    with_servers(linear, said_of)
}

/// A Claude chat on a turn, where one of the hub's has been edited so that its agent connects to it again once that ends.
pub(in super::super) fn chat_servers_edited() -> World {
    let linear = named("Linear", tool_server("linear"));
    let waits = ToolServerState {
        connects_again: Some(ConnectsAgain::WhenItsTurnEnds),
        ..with_tools("Linear", Some("linear"), 23)
    };
    let said_of = vec![waits, with_tools("Notion", Some("notion"), 14), with_tools("Runbooks", Some("runbooks"), 2)];
    let mut world = with_servers(linear, said_of);
    world.sessions.last_mut().unwrap().status = SessionStatus::Running;
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
    let chat = world.sessions.last_mut().unwrap();
    chat.agent_capabilities = running(AgentKind::Claude);
    let chat = chat.id.clone();
    world.observe(observed::tool_servers(chat, said_of));
    world
}

/// What `agent`'s run claims, which says what can be reloaded.
fn running(agent: AgentKind) -> Option<RunCapabilities> {
    let description = agents::descriptions().into_iter().find(|description| description.kind == agent)?;
    Some(RunCapabilities::new(RunId::new(1), description.capabilities))
}

fn named(name: &str, server: ToolServer) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..server }
}

fn said(name: &str, hubs: Option<&str>, state: ServerState) -> ToolServerState {
    ToolServerState {
        name: name.into(),
        hubs: hubs.map(Into::into),
        connected_to: None,
        state,
        error: None,
        tools: None,
        connects_again: None,
    }
}

fn with_tools(name: &str, hubs: Option<&str>, tools: u32) -> ToolServerState {
    ToolServerState { tools: Some(tools), ..said(name, hubs, ServerState::Connected) }
}
