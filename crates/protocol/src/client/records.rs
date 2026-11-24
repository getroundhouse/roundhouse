//! The requests that add a repository, and read the hub's records: its repositories, a session's subagents,
//! proposals or how its replies were heard, or every record at once.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Proposal, Records, ReplyHeard, Repo, Rev, SessionId, Subagent};

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

/// Every repository, workspace, session and comment, read at one moment, with the revision it was read at, in the
/// forms a client of version 4 reads. A client follows the hub's level with
/// [`Subscribe`](super::subscriptions::Subscribe), which lists them as it starts, so no client of this build sends it.
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

/// How each of the session's replies said out loud ended, which the feed then says change, as [`SessionSubagents`]
/// lists its subagents, since only the sessions a view shows need them. A hub older than this refuses it as
/// unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRepliesHeard {
    pub id: SessionId,
}

impl Request for SessionRepliesHeard {
    type Reply = RepliesHeard;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionRepliesHeard(self)
    }
}

/// How each of a session's replies said out loud ended, in no order, since each is read by its reply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepliesHeard {
    pub replies: Vec<ReplyHeard>,
    /// The hub's newest revision when they were read, as a list of every record carries.
    pub rev: Rev,
}

impl Reply for RepliesHeard {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::RepliesHeard(heard) = response else { return Err(response) };
        Ok(heard)
    }
}
