//! Registering Roundhouse with an authorization server that doesn't take its client document, by RFC 7591, for a sign-in
//! the person has chosen to make. MCP's spec of 2026-07-28 deprecates it in favour of the document, so it's here only for
//! services that don't take the document yet, and goes once they do: this module, what `Takes` says of it, the
//! registration a `Pending` and a `SignedIn` have, and what the hub does with that, which is to keep it beside the
//! sign-in, and its `InDoubt`.
//!
//! Roundhouse registers only as a client with no secret. A service may have no way to delete a registration, so one is
//! kept and used again, until the service turns out to have forgotten it.

#[cfg(test)]
mod tests;

mod answer;
pub(crate) use answer::NO_SECRET;
mod client;
pub use client::{Client, TakenAs};
mod client_metadata;
pub(crate) use client_metadata::client_metadata;
mod registration;
pub use registration::Registration;
