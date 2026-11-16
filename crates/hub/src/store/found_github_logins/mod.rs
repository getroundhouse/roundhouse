//! Who each provider's machines' own `gh` is logged in to github.com as, as the hub found it since it started, a row
//! for each provider, which the hub deletes as it starts.

#[cfg(test)]
use protocol::model::{GhLogin, GhLoginWorks};

#[cfg(test)]
mod tests;

mod rows;
pub(super) use rows::found_github_logins;
