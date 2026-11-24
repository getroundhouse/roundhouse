//! What reading workspaces' pull requests needs of GitHub, from their records and their waits, and what they come to
//! when it can't be had.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Branch, Branches, Workspace, WorkspaceId};
use tracing::warn;

use super::judging::{Wanted, wanted};
use super::scopes::WaitsRead;
use crate::hub::Hub;
use crate::hub::pull_requests::reading::couldnt_read;
use crate::hub::pull_requests::recording::Found;
use crate::hub::pull_requests::remote::{Waited, waited};
use crate::hub::pull_requests::unseen::OwnUnread;
use crate::store::Store;

/// What a read found of one workspace: its own pull request, and what its waits watch, for them to judge.
pub(in crate::hub::pull_requests) struct WorkspaceRead {
    pub(in crate::hub::pull_requests) own: Result<Found, OwnUnread>,
    pub(in crate::hub::pull_requests) waits: Vec<WaitsRead>,
}

impl WorkspaceRead {
    /// Unread, as `error` says, with nothing for its waits.
    pub(in crate::hub::pull_requests) fn unread(error: ApiError) -> Self {
        Self { own: Err(error.into()), waits: Vec::new() }
    }
}

/// What reading one workspace's pull requests asks of GitHub: which of its branch's is its own, as [`wanted`] says,
/// and what its waits watch.
pub(in crate::hub::pull_requests) struct Plan {
    pub(in crate::hub::pull_requests) workspace: Workspace,
    /// The branch its worktree has checked out, as its branches said as `own` was worked out from them.
    pub(in crate::hub::pull_requests) checked_out: Option<Branch>,
    pub(in crate::hub::pull_requests) own: Wanted,
    pub(in crate::hub::pull_requests) waited: Vec<Waited>,
}

impl Plan {
    fn needs_github(&self) -> bool {
        matches!(self.own, Wanted::Heads { .. }) || !self.waited.is_empty()
    }
}

/// What reading workspaces' pull requests needs of GitHub, a plan for each.
pub(in crate::hub::pull_requests) struct Asking(pub(in crate::hub::pull_requests) Vec<Plan>);

impl Asking {
    /// The first of them that needs GitHub, whose worker is asked for a machine's token if one is needed.
    pub(in crate::hub::pull_requests) fn first_asked(&self) -> Option<WorkspaceId> {
        self.0.iter().find(|plan| plan.needs_github()).map(|plan| plan.workspace.id.clone())
    }

    /// Whether any of them needs GitHub, and so a token.
    pub(in crate::hub::pull_requests) fn needs_github(&self) -> bool {
        self.0.iter().any(Plan::needs_github)
    }

    /// What they come to with nothing read, for when none needs GitHub, or else with those that do unread.
    pub(in crate::hub::pull_requests) fn settled(self) -> Vec<(WorkspaceId, WorkspaceRead)> {
        self.failed(&couldnt_read())
    }

    /// The same, where those that need GitHub can't be read, as `error` says, such as with no token to read them as.
    /// For their waits it's a read that may work later, whatever it is for the workspace's own, since a login can come
    /// back, and the default can change, where only GitHub losing a pull request ends a wait on it.
    pub(in crate::hub::pull_requests) fn failed(self, error: &ApiError) -> Vec<(WorkspaceId, WorkspaceRead)> {
        let for_waits = ApiError::new(ErrorCode::Worker, error.message.clone());
        let read = |Plan { workspace, own, waited, .. }: Plan| {
            let own = match own {
                Wanted::Nothing { on } => Ok(Found::nothing(on)),
                Wanted::NotKnown => Err(OwnUnread::NotKnown),
                Wanted::Unreadable { error, .. } => Err(error.into()),
                Wanted::Heads { .. } => Err(error.clone().into()),
            };
            let waits =
                waited.into_iter().map(|waited| WaitsRead::failed(waited.repository, for_waits.clone())).collect();
            (workspace.id, WorkspaceRead { own, waits })
        };
        self.0.into_iter().map(read).collect()
    }
}

impl Hub {
    /// What reading the pull requests of each of `workspaces` needs of GitHub: its own, as [`wanted`] says of its
    /// branches as the hub knows them, and those its waits watch.
    pub(in crate::hub::pull_requests) async fn asking(&self, workspaces: Vec<Workspace>) -> Asking {
        let waits = self.blocking(Store::remote_waits).await.unwrap_or_else(|e| {
            warn!("reading which waits to read the pull requests of: {}", e.message);
            Vec::new()
        });
        // With what each last read, whose stack names the lower pull requests it watches too.
        let mut waited = waited(waits.into_iter().map(|wait| self.as_read(wait)).collect());
        let mut plans = Vec::new();
        for workspace in workspaces {
            let address = self.repo_of(&workspace).await.ok().and_then(|repo| repo.origin.url().cloned());
            let branches = self.branches_of(&workspace.id);
            let own = wanted(branches.as_ref(), address.as_ref());
            let checked_out = branches.as_ref().and_then(Branches::checked_out).cloned();
            let waited = waited.remove(&workspace.id).unwrap_or_default();
            plans.push(Plan { workspace, checked_out, own, waited });
        }
        Asking(plans)
    }
}
