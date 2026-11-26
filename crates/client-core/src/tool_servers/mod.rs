//! What every client says of MCP servers: each one a chat has, in the composer's card, and the person's own name for one
//! of the hub's where its agent's name for it would show.

#[cfg(test)]
mod tests;

mod chat;
mod names;
mod reload;
mod sign_in;
mod stands;

pub use chat::{ChatToolServer, ChatToolServers, OwnToolServers};
pub use names::ToolServerNames;
pub use reload::{Offered, Reload, ReloadRequest};
pub use sign_in::{SignIn, SignInFor, SignInStands};
pub use stands::{Stands, Tone};
