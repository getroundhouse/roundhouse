//! Who each provider's machines' own `gh` is logged in to github.com as, as the hub found it since it started, a row
//! for each provider, which the hub deletes as it starts. They're rows, though they last only one run of the hub, so
//! their revisions come from the store's one counter, which orders them with every other record.

#[cfg(test)]
use protocol::model::{GhLogin, GhLoginWorks};

#[cfg(test)]
mod tests;

mod rows;
pub(super) use rows::found_github_logins;
