//! The repositories the hub's GitHub accounts can see, which a client lists for a person to add a project from.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{GithubRepositoryId, GithubRepositoryName, GithubUsername};

/// The page of the repositories the hub's connected GitHub accounts can see, together, whose owner and name have
/// `search` in them, whatever its case, that starts at `after`, or the first, by owner and name. Every account's are
/// in one list, each repository once, with the accounts that can see it. Reading GitHub changes nothing, so it's safe
/// to send again. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct GithubRepositoryList {
    /// Empty for every repository.
    #[serde(default)]
    pub search: String,
    /// Where the page starts, as the page before said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl Request for GithubRepositoryList {
    type Reply = GithubRepositories;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubRepositoryList(self)
    }
}

/// A page of the repositories the hub's GitHub accounts can see.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubRepositories {
    pub repositories: Vec<GithubRepositorySeen>,
    /// Where the next page starts, to send as `after`, or `None` on the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl Reply for GithubRepositories {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubRepositories(repositories) = response else { return Err(response) };
        Ok(repositories)
    }
}

/// A repository the hub's GitHub accounts can see, which adding a project by links to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubRepositorySeen {
    pub id: GithubRepositoryId,
    /// Its owner and name, as GitHub writes them now, such as `getroundhouse/roundhouse`.
    pub repository: GithubRepositoryName,
    pub private: bool,
    /// The accounts that can see it, by who GitHub knows them as, the default's first.
    pub seen_by: Vec<GithubUsername>,
}
