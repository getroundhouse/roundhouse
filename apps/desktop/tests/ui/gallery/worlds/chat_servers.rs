//! The MCP servers a chat's hub has, all made up.

#![cfg(test)]

use client_core::fake_hub::World;
use protocol::event::EventBody;
use protocol::fixtures::tool_server;
use protocol::model::{SessionStatus, ToolServer, ToolServerName};

use super::scenarios::chat_world;

/// A chat of `events`, in `status`, on a hub that has a server the person called `GitHub (Northwind)`.
pub(in super::super) fn chat_with_a_server(status: SessionStatus, events: Vec<EventBody>) -> World {
    let mut world = chat_world(status, events);
    world.tool_servers.push(named("GitHub (Northwind)", tool_server("github")));
    world
}

fn named(name: &str, server: ToolServer) -> ToolServer {
    ToolServer { name: ToolServerName::stored(name.into()), ..server }
}
