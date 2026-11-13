//! Which versions of a protocol each side speaks, and the one two sides use, as "Changing the protocol" in
//! `docs/architecture.md` says. Each protocol has a version of its own: [`crate::client::CLIENT_PROTOCOL_VERSION`],
//! [`crate::worker::WORKER_PROTOCOL_VERSION`] and [`crate::provider::PROVIDER_PROTOCOL_VERSION`].

#[cfg(test)]
mod tests;

mod supported;
pub use supported::Versions;
