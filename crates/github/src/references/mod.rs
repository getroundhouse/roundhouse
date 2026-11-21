//! Reading issues and pull requests from GitHub's GraphQL API by their numbers, as the hub does for those a chat's text
//! links to, so a reply can draw each as a chip that says where it stands: many at once, as one token,
//! each repository's numbers together, in as few queries as GitHub allows.

mod parse;
pub(crate) use parse::pull_request_reference;
mod query;
pub(crate) use query::pull_request_summary;
mod read;
pub use read::ReferencesRead;
#[cfg(test)]
mod tests;
