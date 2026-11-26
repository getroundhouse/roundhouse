//! Signing in to one of the hub's MCP servers from this device: starting it at the hub, opening the server's page in the
//! browser, listening for where that sends the person back, and giving the hub what they're sent back with. It's the
//! window's own rather than that of what started it, so it goes on whatever the window shows, and a hub's view has at
//! most one. Whatever draws it reads how it stands, and starts it, opens its page again or puts it away.
//!
//! The hub is the one that signs in. This device only has the browser, so it's the one that has to listen, and it keeps
//! nothing of what it hears once the hub has answered.

mod page;
mod sent_back;
mod signing_in;
#[cfg(all(test, unix))]
mod tests;
mod under_way;
mod words;

pub(crate) use signing_in::ToolSigningIn;
pub(crate) use under_way::{NewServer, SignInTo, Stands, UnderWay};
