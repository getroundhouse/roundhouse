//! Repositories, each added by its checkout on the hub's machine, with the address a worker there read from it, and
//! its link to its repository on a host.

#[cfg(test)]
mod tests;

mod links;
pub use links::RepoToLink;
mod rows;
pub use rows::RepoToRead;
pub(super) use rows::{RepoColumns, repo_from_row};
