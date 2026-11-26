//! The MCP servers the CLI has, as it says them, and which of them a call that went on in the background is to.

mod known;
mod said;

pub(super) use known::Servers;
pub(super) use said::said;
