//! Signing in to a tool server and renewing that, on a hub that never starts a worker, against a stand-in for the server
//! and its authorization server, on tokio's clock as it runs, since a request to the stand-in races a paused clock. What's
//! kept, and what's refused, is tested in `crate::tool_servers`, and what's asked of a server in the `tool-sign-in` crate.

mod editing;
mod refused;
mod registering;
mod renewals;
mod renewing;
mod sign_ins;
mod support;
mod waiting;
mod wanted;
