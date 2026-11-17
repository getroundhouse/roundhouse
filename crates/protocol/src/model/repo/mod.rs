//! A repository's record: where the repository is, by its checkout on the hub's machine, its address, or both.

mod github;
mod origin;
mod record;
#[cfg(test)]
mod tests;
mod url;

pub use origin::RepoOrigin;
pub use record::Repo;
pub use url::{RepoUrl, RepoUrlError};
