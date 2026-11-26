//! What a server's refusal says, and what's of a sign-in from end to end: renewing one, and that no secret of one is in
//! what's logged.

mod challenge;
mod client_document;
mod renewing;
mod secrets;
mod server_url;
mod signing_in;
pub(crate) use signing_in::{CLIENT, REDIRECT, approved, begun};
