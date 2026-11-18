//! A hub that answers from a fixed [`World`], so what a client shows of it is the same on every run, for a test of
//! what a client draws rather than of what a hub does. It speaks the real protocol over a real socket, so the client
//! is unchanged, and it refuses anything that would change what it holds. The gallery draws every screen from one,
//! and it's here, rather than beside the gallery, which only a Mac builds, so every platform's tests build it, and a
//! change to the protocol that it doesn't follow fails there.
#![cfg(unix)]
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

mod base_pages;
pub mod events;
mod model_lists;
mod serve;
mod stand_in;
mod terminals;
#[cfg(test)]
mod tests;
mod world;

pub use serve::serve;
pub use stand_in::{BUILD, BUILD_NUMBER};
pub use world::{HOUR_MS, HubBuild, LoginSupport, NOW_MS, PairingWorld, World};
