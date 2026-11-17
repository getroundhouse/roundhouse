//! Whether the default GitHub account can push to each repository's repository on GitHub: which one each pushes to, as
//! the first worker of it the hub links to after it starts says, and GitHub's answer, as the hub checks it with the
//! default's login and records it as the repository's `GithubReach`, as that says.

#[cfg(test)]
use checking::{CHECKED_AGAIN_EVERY, Checks};

#[cfg(test)]
mod tests;

mod asking;
mod checking;
mod reaching;
mod remotes;
pub(crate) use remotes::Remotes;
