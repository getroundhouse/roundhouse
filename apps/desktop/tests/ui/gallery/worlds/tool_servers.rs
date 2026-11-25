//! The tool servers the hub has of its own, as MCP servers in Settings lists them. Their keys are the hub's, so none is
//! here.

use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::tool_server;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerName, ToolServerUrl};

use super::conversation;
use client_core::fake_hub::World;

/// The conversation, on a hub with four servers: two accounts at one URL, each with a key, one anyone can use, and one
/// of a company's own.
pub(in crate::gallery) fn tool_servers() -> World {
    let mut world = conversation();
    let docs = ToolServer { auth: ToolServerAuth::Neither, ..server("Docs", "https://docs.northwind.example/mcp") };
    world.tool_servers = vec![
        docs,
        linear(),
        server("Linear (Northwind)", linear().url.as_str()),
        server("Runbooks", "https://mcp.northwind.example/mcp"),
    ];
    world
}

/// The conversation, on a hub with one server.
pub(in crate::gallery) fn one_tool_server() -> World {
    let mut world = conversation();
    world.tool_servers = vec![linear()];
    world
}

/// [`one_tool_server`], whose hub refuses to add another of its name, as it words why.
pub(in crate::gallery) fn tool_server_refused() -> World {
    let mut world = one_tool_server();
    let why = format!("another MCP server is called {}", linear().name);
    world.tool_server_add_refused = Some(ApiError::new(ErrorCode::Conflict, why));
    world
}

fn linear() -> ToolServer {
    server("Linear", "https://mcp.linear.app/mcp")
}

/// The server called `name`, at `url`, which a key is sent to.
fn server(name: &str, url: &str) -> ToolServer {
    let id = name.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "");
    ToolServer {
        name: ToolServerName::stored(name.to_owned()),
        url: ToolServerUrl::stored(url.to_owned()),
        ..tool_server(&id)
    }
}
