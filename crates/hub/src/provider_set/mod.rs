//! The providers a hub runs workspaces on, by the name each workspace records, and the one new workspaces go to.
//!
//! The local provider is always there. *Not built:* naming the others in the hub's settings, as
//! `docs/spec/workspaces.md` has it. Until then a hub reads them from `providers.json` in its data directory, as it
//! starts:
//!
//! ```json
//! {"default": "incus", "providers": [{"name": "incus", "program": "/path/to/roundhouse-incus", "args": []}]}
//! ```
//!
//! An entry can also say `"set_up_github": false`, so Roundhouse leaves GitHub on that provider's machines to the
//! machine, as it always does on the local provider's, as `ProviderSet::leaves_github_to_machine` says.

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
