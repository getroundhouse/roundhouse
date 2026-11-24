//! Every kind of observation, one of each, as the test makes them, from one exhaustive match, with where a client is
//! sent each.

use protocol::model::{
    AgentKind, GithubAccountId, GithubAppId, GithubRepositoryName, IssueNumber, ModelListKey, ObservationKey,
    ReferenceAsk, SessionId, WorkspaceId,
};

/// The subscriptions' IDs: one that follows the hub, the workspace [`v`], the session `s`, and [`chip`].
pub(super) const HUB: u64 = 1;
pub(super) const WORKSPACE: u64 = 2;
pub(super) const SESSION: u64 = 3;
pub(super) const CHIP: u64 = 4;

/// The app of your own the test's hub has, whose installation is the GitHub account [`installation`].
pub(super) const APP: GithubAppId = GithubAppId::new(9);

/// The workspace each kind of a workspace is of, which has no session, so reading its pull request, as following it at
/// its level does, judges no wait, and leaves alone what the test makes the wait `a` of the session `s`, in `w`, read.
pub(super) fn v() -> WorkspaceId {
    "v".into()
}

pub(super) fn w() -> WorkspaceId {
    "w".into()
}

pub(super) fn s() -> SessionId {
    "s".into()
}

/// The GitHub account the test's hub has, an installation of [`APP`].
pub(super) fn installation() -> GithubAccountId {
    "g".into()
}

/// The issue a chip follows.
pub(super) fn chip() -> ReferenceAsk {
    ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(12).unwrap(),
    }
}

/// The list of models whose catalog the test makes.
pub(super) fn models() -> ModelListKey {
    ModelListKey {
        agent: AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    }
}

/// One observation of every kind a client can be sent, each of what the test makes it of.
pub(super) fn every_kind() -> Vec<ObservationKey> {
    std::iter::successors(Some(ObservationKey::WorkspaceLines { workspace_id: v() }), after).collect()
}

/// The kind after `kind`, in a match that's exhaustive, so a kind added fails to compile here until the test makes one
/// of it, as it does in `making` and [`sent_at`].
fn after(kind: &ObservationKey) -> Option<ObservationKey> {
    use ObservationKey as K;
    Some(match kind {
        K::WorkspaceLines { .. } => K::WorkspaceChanges { workspace_id: v() },
        K::WorkspaceChanges { .. } => K::WorkspacePullRequest { workspace_id: v() },
        K::WorkspacePullRequest { .. } => K::WorkspacePullRequestDetail { workspace_id: v() },
        K::WorkspacePullRequestDetail { .. } => K::WorkspaceRepositoryUnseen { workspace_id: v() },
        K::WorkspaceRepositoryUnseen { .. } => K::Reference { reference: chip() },
        K::Reference { .. } => K::WorkspaceBranches { workspace_id: v() },
        K::WorkspaceBranches { .. } => K::WorkspaceWorkerBuild { workspace_id: v() },
        K::WorkspaceWorkerBuild { .. } => K::RepositoryReach { repo: "r".into() },
        K::RepositoryReach { .. } => K::GithubUsername { account: installation() },
        K::GithubUsername { .. } => K::GithubAppSaid { app: APP },
        K::GithubAppSaid { .. } => K::GithubAppBot { app: APP },
        K::GithubAppBot { .. } => K::InstalledOn { account: installation() },
        K::InstalledOn { .. } => K::ModelCatalog { list: models() },
        K::ModelCatalog { .. } => K::AccountIdentity { account: "claude-ada".into() },
        K::AccountIdentity { .. } => K::AccountLimits { account: "claude-ada".into() },
        K::AccountLimits { .. } => K::SessionContext { session_id: s() },
        K::SessionContext { .. } => K::SessionTodos { session_id: s() },
        K::SessionTodos { .. } => K::SessionCache { session_id: s() },
        K::SessionCache { .. } => K::WaitRead { session_id: s(), wait: "a".into() },
        K::WaitRead { .. } => K::ReleaseOffered,
        K::ReleaseOffered | K::Unknown => return None,
    })
}

/// The subscription a client is sent `kind` on, at its level, or `None` for a kind sent to no client.
pub(super) fn sent_at(kind: &ObservationKey) -> Option<u64> {
    use ObservationKey as K;
    Some(match kind {
        K::WorkspaceLines { .. }
        | K::WorkspacePullRequest { .. }
        | K::WorkspaceWorkerBuild { .. }
        | K::RepositoryReach { .. }
        | K::GithubUsername { .. }
        | K::InstalledOn { .. }
        | K::GithubAppSaid { .. }
        | K::GithubAppBot { .. }
        | K::ModelCatalog { .. }
        | K::AccountIdentity { .. }
        | K::AccountLimits { .. }
        | K::SessionCache { .. }
        | K::ReleaseOffered => HUB,
        K::WorkspaceChanges { .. }
        | K::WorkspacePullRequestDetail { .. }
        | K::WorkspaceRepositoryUnseen { .. }
        | K::WorkspaceBranches { .. } => WORKSPACE,
        K::Reference { .. } => CHIP,
        K::SessionContext { .. } | K::SessionTodos { .. } | K::WaitRead { .. } => SESSION,
        // Of a kind this build doesn't know, which the hub never makes.
        K::Unknown => return None,
    })
}
