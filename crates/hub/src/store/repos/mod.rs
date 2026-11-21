//! Repositories, each added by its checkout on the hub's machine, with the address a worker there read from it.

#[cfg(test)]
mod tests;

mod rows;
pub use rows::RepoToRead;
pub(super) use rows::{RepoColumns, repo_from_row};
