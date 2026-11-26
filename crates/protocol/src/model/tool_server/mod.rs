//! The tool servers the hub has of its own, which are MCP servers over HTTP: what each is called, where it is, and how
//! it knows the person.

#[cfg(test)]
mod tests;

mod auth;
mod header;
mod name;
mod place;
mod record;
mod scopes;
mod url;

pub use auth::{ToolServerAuth, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey, ToolSignInStatus};
pub use header::{ToolServerHeader, ToolServerHeaderError};
pub use name::{ToolServerName, ToolServerNameError};
pub use place::ToolServerPlace;
pub use record::ToolServer;
pub use scopes::ToolScopes;
pub use url::{ToolServerUrl, ToolServerUrlError};
