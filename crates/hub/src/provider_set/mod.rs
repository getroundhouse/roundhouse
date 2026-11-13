//! The providers a hub runs workspaces on, by the name each workspace records, and the one new workspaces go to.
//!
//! The local provider is always there. *Not built:* naming the others in the hub's settings, as "Providers" in
//! `docs/architecture.md` has it. Until then a hub reads them from `providers.json` in its data directory, as it
//! starts:
//!
//! ```json
//! {"default": "incus", "providers": [{"name": "incus", "program": "/path/to/roundhouse-incus", "args": []}]}
//! ```

#[cfg(test)]
use protocol::Build;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use providers::Provider;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod configuration;
pub(crate) use configuration::ProviderSet;
