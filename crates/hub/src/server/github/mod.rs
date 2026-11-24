//! Feeding a subscription of a client of version 4 that doesn't read GitHub as observations what GitHub says of what it
//! follows, as `HubFrame::Github`: what the hub knows of each, sent as it changes.

mod pumping;
pub(super) use pumping::{GithubShown, pump};
#[cfg(test)]
mod tests;
