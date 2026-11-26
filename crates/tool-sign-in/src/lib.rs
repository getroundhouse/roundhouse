//! What the hub asks MCP servers itself. Signing in to one as an OAuth client, by MCP's authorization spec of
//! 2026-07-28: finding out whether a server takes a sign-in and where, the page a person approves it on, the code for
//! tokens, and renewing them. And asking one for its tools, with a key or with none, to see whether it's there and
//! takes the key, as [`ToolSignIn::list_tools`] says. Both are here since both begin by asking a server something, as
//! MCP has it asked.
//!
//! Roundhouse is the client that its document at [`CLIENT_DOCUMENT`] describes, or one it registers as where an
//! authorization server doesn't take that, as [`ToolSignIn::client_for`] says. Each call is within a time limit, and of
//! the addresses that servers name, fetches only what [`NotFetched`] leaves. It keeps nothing, and knows nothing of the
//! hub, which is to keep what the calls return, as `docs/decisions/hub-holds-tool-servers.md` says. What a server's
//! answer says of signing in and of where it sends a request on is read by [`protocol::tool_server_http`], as a worker
//! reads it too.

#![warn(clippy::wildcard_enum_match_arm)]

mod checking;
mod client;
pub use client::{CLIENT_DOCUMENT, ToolSignIn};
mod client_id;
pub use client_id::ClientId;
mod failure;
pub use failure::{Failure, OauthError, shown};
mod fetch;
pub use fetch::NotFetched;
mod finding;
pub use finding::{Found, Issuer, NoWay, Way};
mod messages;
mod offline_access;
mod registering;
pub use registering::{Client, Registration, TakenAs};
mod renewing;
pub use renewing::{Renewable, Renewal};
mod secrets;
pub use secrets::{AccessToken, AuthorizationCode, AuthorizationPage, RefreshToken, SignInState};
mod server_url;
pub use server_url::{NotAServerUrl, ServerUrl};
mod signing_in;
pub use signing_in::{NotARedirect, Pending, RedirectUri, Returned, SignedIn, Started};
mod token_endpoint;
pub use token_endpoint::Granted;
#[cfg(any(test, feature = "testing"))]
mod stand_in;
#[cfg(any(test, feature = "testing"))]
pub use stand_in::{
    Asked, Asks, Gate, Iss, MetadataAt, Person, Query, Refreshing, Registers, Says, SaysAllowed, Script,
    ServerMetadataAt, StandIn, Wants,
};
#[cfg(test)]
mod tests;
