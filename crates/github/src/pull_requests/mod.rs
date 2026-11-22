//! Reading pull requests from GitHub's GraphQL API, as the hub does for the pull requests agents ask for and wait on:
//! each with its checks and which of them are required, and those open under it in its stack, many at once, as one
//! token, in as few queries as GitHub allows.

mod asked;
mod facets;
pub use asked::{FacetsRead, PullRequestsAsk, PullRequestsRead, RulesRead, SummaryRead, Unread};
pub use facets::{Checks, PullFacets, Rules, judged, observe};
mod graphql;
mod heads;
pub use heads::{BRANCHES_A_QUERY, Headed, HeadsAsk, HeadsRead};
mod parse;
pub use parse::ReportedCheck;
pub(crate) use parse::{queue, review, time};
mod query;
pub(crate) use query::named;
mod reading;
mod rounds;
pub use rounds::Leaving;
pub(crate) use rounds::Spent;
mod said;
pub use said::RateLimit;
pub(crate) use said::{array, failed, incomplete, string};
#[cfg(test)]
mod tests;
