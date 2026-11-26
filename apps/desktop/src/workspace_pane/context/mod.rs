//! What sits beside Send: a plug for the chat's MCP servers, a bar for each of the account's limits, and a ring for how
//! full the chat's context is. Each shows a card while the pointer rests on it: the servers, how much of the limits is
//! left and whose account it is, with a menu that moves the chat to another account, or what the context holds.

mod card;
mod moving;
mod tool_servers;
mod words;

mod usage;

pub(super) use moving::line_button;
pub(super) use usage::Usage;
