//! The tool servers the hub has of its own, as MCP servers in Settings lists them. Their sign-ins and keys are the
//! hub's, so none is here.

use protocol::client::tool_sign_ins::{NoToolSignIn, ToolSignInBegun, ToolSignInPage};
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::tool_server;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerName, ToolServerUrl, ToolSignInStatus};

use super::conversation;
use client_core::fake_hub::World;

/// The conversation, on a hub with four servers: two accounts at one URL, each signed in to, one of them once and no
/// longer, one anyone can use, and one of a company's own, with a key.
pub(in crate::gallery) fn tool_servers() -> World {
    let mut world = conversation();
    let docs = ToolServer { auth: ToolServerAuth::Neither, ..server("Docs", "https://docs.northwind.example/mcp") };
    let needs_sign_in = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn };
    world.tool_servers = vec![
        docs,
        ToolServer { auth: needs_sign_in, ..linear() },
        ToolServer { auth: linear().auth, ..server("Linear (Northwind)", linear().url.as_str()) },
        server("Runbooks", "https://mcp.northwind.example/mcp"),
    ];
    world
}

/// The conversation, on a hub with one server, which is signed in to.
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

/// The page to approve a sign-in on.
pub(super) fn page() -> ToolSignInBegun {
    ToolSignInBegun::Page { page: ToolSignInPage::new("https://auth.notion.example/authorize?state=made-up") }
}

/// [`one_tool_server`], whose hub answers a sign-in to another with the page to approve it on.
pub(in crate::gallery) fn tool_sign_in_waiting() -> World {
    let mut world = one_tool_server();
    world.tool_sign_in_answer = Some(Ok(page()));
    world
}

/// [`one_tool_server`], whose hub answers a sign-in to another that there's none.
pub(in crate::gallery) fn no_tool_sign_in() -> World {
    let begun = ToolSignInBegun::NoWay { why: NoToolSignIn::NoAuthorizationServer };
    let mut world = one_tool_server();
    world.tool_sign_in_answer = Some(Ok(begun));
    world
}

/// [`one_tool_server`], whose hub answers that the server to sign in to is one anyone can use.
pub(in crate::gallery) fn open_tool_server() -> World {
    let mut world = one_tool_server();
    world.tool_sign_in_answer = Some(Ok(ToolSignInBegun::NoSignIn));
    world
}

/// [`tool_servers`], whose hub can't reach a server to sign in to it, as it words why.
pub(in crate::gallery) fn tool_sign_in_refused() -> World {
    let why = ApiError::new(ErrorCode::Unavailable, "mcp.linear.app didn't answer in time");
    let mut world = tool_servers();
    world.tool_sign_in_answer = Some(Err(why));
    world
}

fn linear() -> ToolServer {
    let works = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };
    ToolServer { auth: works, ..server("Linear", "https://mcp.linear.app/mcp") }
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
