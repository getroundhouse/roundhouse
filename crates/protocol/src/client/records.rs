//! The requests that add a repository, and read the hub's records: its repositories, a session's subagents or
//! proposals, or every record at once.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Proposal, Records, Repo, Rev, SessionId, Subagent};

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

/// Every repository, workspace, session and comment, read at one moment, with the revision it was read at. Follow
/// the feed first, with [`FeedSubscribe`](super::subscriptions::FeedSubscribe), then list, and keep whichever copy of
/// a record is newer.
#[derive(Clone, Copy, Debug)]
pub struct RecordsList;

impl Request for RecordsList {
    type Reply = Records;

    fn into_request(self) -> ClientRequest {
        ClientRequest::RecordsList
    }
}

impl Reply for Records {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Records(records) = response else { return Err(response) };
        Ok(records)
    }
}

/// The records of the session's subagents, which the feed then says change, as [`RecordsList`] lists every other
/// record. They're listed by session rather than with the rest, since each holds what it was asked and its report, in
/// full. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionSubagents {
    pub id: SessionId,
}

impl Request for SessionSubagents {
    type Reply = Subagents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionSubagents(self)
    }
}

/// A session's subagents, oldest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Subagents {
    pub subagents: Vec<Subagent>,
    /// The hub's newest revision when they were read, as a list of every record carries.
    pub rev: Rev,
}

impl Reply for Subagents {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Subagents(subagents) = response else { return Err(response) };
        Ok(subagents)
    }
}

/// The session's proposals, which the feed then says change, as [`SessionSubagents`] lists its subagents, and for the
/// same reason: each holds what its agent wrote, in full. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionProposals {
    pub id: SessionId,
}

impl Request for SessionProposals {
    type Reply = Proposals;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionProposals(self)
    }
}

/// A session's proposals, oldest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Proposals {
    pub proposals: Vec<Proposal>,
    /// The hub's newest revision when they were read, as a list of every record carries.
    pub rev: Rev,
}

impl Reply for Proposals {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Proposals(proposals) = response else { return Err(response) };
        Ok(proposals)
    }
}
