//! Whether the default GitHub account can push to each repository's repository on GitHub, as the hub last checked, a
//! row for each repository, with when GitHub answered, kept across restarts, so a hub that starts again has it at once,
//! as stale until it checks again, and knows which repository on GitHub to check.

#[cfg(test)]
mod tests;

mod rows;
pub(super) use rows::github_reach;
