//! What each kind of observation can be of, and which of the hub's records it goes with, so the store keeps it only
//! while that record is there.

use std::hash::Hash;

use protocol::model::{
    AccountId, ChildWorktreeId, GithubAccountId, GithubAppId, ModelListKey, ReferenceAsk, RepoId, SessionId, WaitId,
    WorkspaceId,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::store::Record;

/// What an observation can be of, which says which of the hub's records it goes with, if any, so the store keeps it
/// only while that record is there, whatever its kind, and a new kind of what's of a workspace, a session or an
/// account needs nothing more to go with it.
pub(crate) trait Subject: Clone + Eq + Hash + Serialize + DeserializeOwned + Send + Sync {
    fn goes_with(&self) -> Option<GoesWith<'_>>;
}

/// Which of the hub's records an observation goes with: nothing of it is kept while the record isn't there, as the
/// database says as the change takes its revision, and it's forgotten once the record goes, as a workspace is removed,
/// and as the hub starts on a database that lacks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoesWith<'a> {
    Workspace(&'a WorkspaceId),
    /// A workspace's own worktree, which goes with its workspace, and as it's archived, and is back once it's
    /// restored.
    Worktree(&'a WorkspaceId),
    /// Another worktree made for a workspace's work, which goes with its workspace too.
    ChildWorktree(&'a ChildWorktreeKey),
    /// A session, which goes with its workspace too.
    Session(&'a SessionKey),
    /// A wait, which goes with its session and its workspace too, and as it ends.
    Wait(&'a WaitKey),
    Account(&'a AccountId),
    Repo(&'a RepoId),
    GithubAccount(&'a GithubAccountId),
    /// An app of your own, forgotten as it's removed.
    App(&'a GithubAppId),
    /// The hub itself, for what's of no record, which is never gone, and goes only as the hub lets go of it.
    Hub,
}

impl<'a> GoesWith<'a> {
    /// The workspace it is, or goes with, if any.
    pub(crate) fn workspace(self) -> Option<&'a WorkspaceId> {
        match self {
            GoesWith::Workspace(workspace) | GoesWith::Worktree(workspace) => Some(workspace),
            GoesWith::ChildWorktree(worktree) => Some(&worktree.workspace_id),
            GoesWith::Session(session) => Some(&session.workspace_id),
            GoesWith::Wait(wait) => Some(&wait.workspace_id),
            GoesWith::Account(_)
            | GoesWith::Repo(_)
            | GoesWith::GithubAccount(_)
            | GoesWith::App(_)
            | GoesWith::Hub => None,
        }
    }

    /// The record the database says is there or not, or `None` for the hub's own, which always is.
    pub(super) fn record(self) -> Option<Record> {
        Some(match self {
            GoesWith::Workspace(id) => Record::Workspace(id.clone()),
            GoesWith::Worktree(id) => Record::Worktree(id.clone()),
            GoesWith::ChildWorktree(key) => Record::ChildWorktree(key.id.clone()),
            GoesWith::Session(key) => Record::Session(key.session_id.clone()),
            GoesWith::Wait(key) => Record::Wait(key.wait.clone()),
            GoesWith::Account(id) => Record::Account(id.clone()),
            GoesWith::Repo(id) => Record::Repo(id.clone()),
            GoesWith::GithubAccount(id) => Record::GithubAccount(id.clone()),
            GoesWith::App(id) => Record::App(*id),
            GoesWith::Hub => return None,
        })
    }
}

impl Subject for WorkspaceId {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Workspace(self))
    }
}

/// A chip's issue or pull request, which goes with none of the hub's records, since chips in any chat can name it.
impl Subject for ReferenceAsk {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        None
    }
}

/// A list of models, which the hub keeps for whichever agent, project, provider and account a chat would launch with,
/// and lets go of itself, so it's the hub's.
impl Subject for ModelListKey {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Hub)
    }
}

/// Another worktree made for a workspace's work, as what's observed of it is kept by: with its workspace, so it's
/// forgotten with it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ChildWorktreeKey {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) id: ChildWorktreeId,
}

impl Subject for ChildWorktreeKey {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::ChildWorktree(self))
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
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Session(self))
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
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Wait(self))
    }
}

/// What a hub has at most one of, such as the release the update feed offers it, which is the hub's.
impl Subject for () {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Hub)
    }
}

impl Subject for AccountId {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Account(self))
    }
}

impl Subject for RepoId {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::Repo(self))
    }
}

impl Subject for GithubAccountId {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::GithubAccount(self))
    }
}

impl Subject for GithubAppId {
    fn goes_with(&self) -> Option<GoesWith<'_>> {
        Some(GoesWith::App(self))
    }
}
