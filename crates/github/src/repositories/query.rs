//! The GraphQL the hub reads repositories with: each ask under an alias of its own, `g` and its place in the query, so
//! one query carries many, and each part of the answer, and each error GitHub gives, finds its way back by its alias.

use protocol::model::{GithubRepositoryId, GithubRepositoryName};

use crate::pull_requests::named;

/// The repositories one query asks for at most. Each is one object of four fields, so a query of these costs GitHub's
/// limit a point.
pub(super) const ASKS_A_QUERY: usize = 100;

/// What's read of each repository: GitHub's ID for it, what it's called, and where git reaches it.
const READ: &str = "id nameWithOwner url sshUrl";

/// Which repository to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryAsk {
    /// The one GitHub has by this owner and name now, as an address names it, to find its ID by.
    Named(GithubRepositoryName),
    /// The one GitHub has by this ID, whatever it's called now.
    Id(GithubRepositoryId),
}

/// One query asking for each of `asks`, and what's left of GitHub's limit.
pub(super) fn query(asks: &[RepositoryAsk]) -> String {
    let asked: Vec<String> = asks
        .iter()
        .enumerate()
        .map(|(at, ask)| match ask {
            RepositoryAsk::Named(repository) => {
                format!("{}: repository({}) {{ {READ} }}", alias(at), named(repository))
            }
            RepositoryAsk::Id(id) => {
                let id = serde_json::Value::String(id.as_str().to_owned());
                format!("{}: node(id:{id}) {{ ... on Repository {{ {READ} }} }}", alias(at))
            }
        })
        .collect();
    format!("query {{ {} rateLimit {{ limit remaining resetAt }} }}", asked.join(" "))
}

/// The alias of the ask `at` its place in the query.
pub(super) fn alias(at: usize) -> String {
    format!("g{at}")
}
