//! What each kind of observation can be of, and what owns it, so the store forgets it with what owns it.

use std::hash::Hash;

use protocol::model::{AccountId, GithubAccountId, ModelListKey, ReferenceAsk, RepoId, SessionId, WaitId, WorkspaceId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// What an observation can be of, which says what owns it, if anything, so the store forgets it with what owns it
/// whatever its kind, and a new kind of what's of a workspace, a session or an account needs nothing more to be
/// forgotten with it.
pub(crate) trait Subject: Clone + Eq + Hash + Serialize + DeserializeOwned + Send + Sync {
    fn owner(&self) -> Option<Owner<'_>>;
}

/// What owns an observation, so it's forgotten once that's gone: as a workspace is removed, and as the hub starts on a
/// database that lacks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner<'a> {
    Workspace(&'a WorkspaceId),
    /// A session, which goes with its workspace too.
    Session(&'a SessionKey),
    /// A wait, which goes with its session and its workspace too, and as it ends.
    Wait(&'a WaitKey),
    Account(&'a AccountId),
    Repo(&'a RepoId),
    GithubAccount(&'a GithubAccountId),
    /// The hub itself, for what's of no record, which is never gone, and goes only as the hub lets go of it.
    Hub,
}

impl<'a> Owner<'a> {
    /// The workspace it is, or goes with, if any.
    pub(crate) fn workspace(self) -> Option<&'a WorkspaceId> {
        match self {
            Owner::Workspace(workspace) => Some(workspace),
            Owner::Session(session) => Some(&session.workspace_id),
            Owner::Wait(wait) => Some(&wait.workspace_id),
            Owner::Account(_) | Owner::Repo(_) | Owner::GithubAccount(_) | Owner::Hub => None,
        }
    }
}

impl Subject for WorkspaceId {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Workspace(self))
    }
}

/// A chip's issue or pull request, which nothing owns, since chips in any chat can name it.
impl Subject for ReferenceAsk {
    fn owner(&self) -> Option<Owner<'_>> {
        None
    }
}

/// A list of models, which the hub keeps for whichever agent, project, provider and account a chat would launch with,
/// and lets go of itself, so it's the hub's.
impl Subject for ModelListKey {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Hub)
    }
}

/// A session, as what's observed of it is kept by: with its workspace, which it never leaves, so it's forgotten with
/// either.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct SessionKey {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) session_id: SessionId,
}

impl Subject for SessionKey {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Session(self))
    }
}

/// A wait, as what's observed of it is kept by: with its session and workspace, so it's forgotten with either.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct WaitKey {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) session_id: SessionId,
    pub(crate) wait: WaitId,
}

impl Subject for WaitKey {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Wait(self))
    }
}

impl Subject for AccountId {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Account(self))
    }
}

impl Subject for RepoId {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::Repo(self))
    }
}

impl Subject for GithubAccountId {
    fn owner(&self) -> Option<Owner<'_>> {
        Some(Owner::GithubAccount(self))
    }
}
