//! The requests that add a repository, and read the hub's repositories.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::Repo;

/// Adds the repository whose checkout on the hub's machine `path` is in, and has a worker there read the address its
/// `origin` fetches from, which the feed then tells of, as the [`Repo`] has it. Adding the same repository again
/// returns the first record, and has its address read again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepoAdd {
    pub path: String,
}

impl Request for RepoAdd {
    type Reply = Repo;

    fn into_request(self) -> ClientRequest {
        ClientRequest::RepoAdd(self)
    }
}

impl Reply for Repo {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Repo(repo) = response else { return Err(response) };
        Ok(repo)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RepoList;

impl Request for RepoList {
    type Reply = Repos;

    fn into_request(self) -> ClientRequest {
        ClientRequest::RepoList
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Repos {
    pub repos: Vec<Repo>,
}

impl Reply for Repos {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Repos(repos) = response else { return Err(response) };
        Ok(repos)
    }
}
