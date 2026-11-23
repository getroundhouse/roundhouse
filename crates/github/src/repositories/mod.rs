//! Reading repositories from GitHub's GraphQL API, as the hub links a project to one and follows it: which repository
//! an owner and a name are now, and what one is called now by GitHub's ID for it, many at once, as one token, in as few
//! queries as GitHub allows, and which repositories a token's user can see, a page at a time.

mod query;
pub use query::RepositoryAsk;
mod read;
pub use read::{RepositoriesRead, RepositoryFound};
mod seen;
pub use seen::{Seen, SeenPage};
#[cfg(test)]
mod tests;
