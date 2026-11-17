//! Which repository on github.com a repository pushes to, as
//! [`GithubRemote`](protocol::worker::GithubRemote) says.

#[cfg(test)]
mod tests;

mod address;
mod pushes_to;
pub use pushes_to::github_remote;
