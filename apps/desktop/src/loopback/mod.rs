//! Listening on `127.0.0.1` for a browser on this device to be sent back to the app, as GitHub sends it back once an app
//! of your own is made, and an authorization server once a sign-in to an MCP server is approved. What's listened for is
//! the caller's to say: it's asked what to answer each request, and says which one is what it listened for, which ends
//! the listener. It listens only on loopback, so nothing on another machine can reach it, and it stops once it has heard
//! that, once whatever started it lets go of it, and after a time limit. A paired device's app listens on its own
//! machine, where the browser is.

mod answering;
mod bound;
mod listening;
pub(crate) mod page;

#[cfg(any(test, feature = "testing"))]
pub mod browsing;
#[cfg(test)]
mod tests;

pub(crate) use answering::{Answer, Asked, Query};
pub(crate) use bound::Bound;
pub(crate) use listening::{Ended, Listening};
