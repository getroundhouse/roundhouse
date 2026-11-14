//! The contracts a session's log can be checked against by itself, so running an adapter's captured fixtures through
//! [`check`] shows that it settles each thing once and records nothing its run doesn't claim, and fails once a CLI
//! upgrade breaks that.
//!
//! The log has to be whole, from its first event, and from a hub that records runs. What has a run is the agent's,
//! but for its launch's start and its exit, which the hub records with the run, and what has none is the hub's. Only
//! what the agent recorded is held to its run's claims, and to its run's end. The hub settles what the agent left
//! open, such as dropping a prompt with a worker it lost, or cancelling a request, and the agent can settle it again
//! after, as when the prompt reaches it after all, which changes nothing, so neither settles anything twice.

mod check;
mod needs;

pub use check::{Breach, Law, check};
pub use needs::needs;

#[cfg(test)]
mod tests;
