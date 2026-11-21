//! Every read of GitHub's issues and pull requests, which goes through the hub's cache, [`crate::github_cache`]: what's
//! asked for is answered from the cache where it's as fresh as the ask needs, and read, once however many ask,
//! where it isn't. Chats' chips read through it, and the reads of pull requests for the bar, waits and agents keep what
//! they read in it, and take their branches' rules from it. And what clients show of them, which the hub sends them as
//! it changes, with the pull requests of archived workspaces they show that the cache has forgotten.

mod forgotten;
pub(crate) use forgotten::Forgotten;
mod reading;
pub(in crate::hub) use reading::RULES_FRESH_FOR;
mod references;
mod shown;
pub(crate) use shown::{ShowingEnds, Shown, ShownBy};
#[cfg(test)]
mod tests;
