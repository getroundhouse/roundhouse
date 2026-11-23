//! The repositories a token's user can see, a page at a time, as the hub lists them for a person to pick one from.

use protocol::client::ApiError;
use protocol::model::GithubToken;
use serde_json::Value;

use super::RepositoryFound;
use super::read::found;
use crate::pull_requests::{Spent, failed, incomplete, string};
use crate::{Github, Leaving};

/// The repositories one page has at most, which is the most GitHub gives a page.
pub(super) const A_PAGE: usize = 100;

/// One page of the repositories a token's user can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenPage {
    pub repositories: Vec<Seen>,
    /// Where the next page starts, or `None` on the last.
    pub next: Option<String>,
}

/// A repository a token's user can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub found: RepositoryFound,
    pub private: bool,
}

impl Github {
    /// The page of repositories `token`'s user can see that starts `after`, or the first: those they own, collaborate
    /// on, or that are an organization's they're a member of, by their owner and name. A read the person waits on, so
    /// it leaves nothing of GitHub's limit.
    pub async fn repositories_seen(&self, token: &GithubToken, after: Option<&str>) -> Result<SeenPage, ApiError> {
        let mut spent = Spent::new(Leaving::Nothing);
        let answer = self.spend(token, &query(after), &mut spent).await?;
        page(&answer)
    }
}

/// The query for the page that starts `after`.
pub(super) fn query(after: Option<&str>) -> String {
    let after = after.map_or_else(String::new, |after| format!(",after:{}", Value::String(after.to_owned())));
    let affiliations = "[OWNER,COLLABORATOR,ORGANIZATION_MEMBER]";
    format!(
        "query {{ viewer {{ repositories(first:{A_PAGE}{after},affiliations:{affiliations},\
         ownerAffiliations:{affiliations},orderBy:{{field:NAME,direction:ASC}}) {{ \
         nodes {{ id nameWithOwner url sshUrl isPrivate }} pageInfo {{ hasNextPage endCursor }} }} }} \
         rateLimit {{ limit remaining resetAt }} }}"
    )
}

/// The page GitHub answered with.
fn page(answer: &Value) -> Result<SeenPage, ApiError> {
    if let Some(error) = answer["errors"].as_array().and_then(|errors| errors.first()) {
        return Err(failed(error));
    }
    let connection = &answer["data"]["viewer"]["repositories"];
    let nodes = connection["nodes"].as_array().ok_or_else(incomplete)?;
    let repositories = nodes
        .iter()
        .map(|node| {
            let found = found(node)?.ok_or_else(incomplete)?;
            Ok(Seen { found, private: node["isPrivate"].as_bool().ok_or_else(incomplete)? })
        })
        .collect::<Result<_, ApiError>>()?;
    let next = match connection["pageInfo"]["hasNextPage"].as_bool() {
        Some(true) => Some(string(&connection["pageInfo"], "endCursor")?),
        Some(false) => None,
        None => return Err(incomplete()),
    };
    Ok(SeenPage { repositories, next })
}
