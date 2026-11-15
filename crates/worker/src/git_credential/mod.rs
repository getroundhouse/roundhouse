//! The git credential a hub sets on its worker, which the worker hands every process it starts with files it rewrites:
//! a git config file, which sends git to the worker's own credential helper for github.com, and a `gh` config
//! directory. So a renewal, or a new default, reaches a running agent's next `git` or `gh`.

mod credentials;
mod files;
mod git_version;
mod helper;
#[cfg(test)]
mod tests;

pub(crate) use credentials::GitCredentials;
pub use helper::{CredentialHelper, answer_credential_request};
