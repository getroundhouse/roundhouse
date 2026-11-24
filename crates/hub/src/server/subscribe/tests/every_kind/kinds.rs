//! Every kind of observation, one of each, as the test makes them, from one exhaustive match, with where a client of
//! version 5 is sent each, and what a client of version 4 is sent in its place.

use protocol::model::{
    AgentKind, GithubAccountId, GithubAppId, GithubRepositoryName, IssueNumber, ModelListKey, ObservationKey, Record,
    RecordKey, ReferenceAsk, SessionId, WorkspaceId,
};

use super::old_forms;

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
/// of it, as it does in `making` and [`sent`].
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

/// Where each client is sent a kind.
pub(super) struct Sent {
    /// The subscription a client of version 5 is sent it on, at its level, or `None` for a kind sent to no client.
    pub(super) at: Option<u64>,
    /// What a client of version 4 is sent in its place, on a start that doesn't say it reads GitHub as observations.
    pub(super) before: Before,
}

/// What a client of version 4 is sent of a kind.
pub(super) enum Before {
    /// The observation itself, at the same level, since no client reads it in another form.
    Itself,
    /// `HubFrame::Github`, as GitHub's, on subscription `on`: the hub's, for a workspace's own pull request, and the
    /// chip's, for its issue.
    GithubUpdate { on: u64 },
    /// A field of `record`, or the whole of it, made from the observation, which `says` reads, as each copy of it says
    /// the round the test made it in, as `making` writes it.
    Record { record: RecordKey, says: fn(&Record) -> Option<String> },
}

/// Where each client is sent `kind`.
pub(super) fn sent(kind: &ObservationKey) -> Sent {
    use ObservationKey as K;
    let (at, before) = match kind {
        K::WorkspaceLines { .. } => (HUB, Before::Itself),
        K::WorkspaceChanges { .. } => (WORKSPACE, Before::Itself),
        K::WorkspacePullRequest { .. } => (HUB, Before::GithubUpdate { on: HUB }),
        K::WorkspacePullRequestDetail { .. } | K::WorkspaceRepositoryUnseen { .. } => {
            (WORKSPACE, Before::GithubUpdate { on: HUB })
        }
        K::Reference { .. } => (CHIP, Before::GithubUpdate { on: CHIP }),
        K::WorkspaceBranches { .. } => (WORKSPACE, on_workspace(old_forms::branches)),
        K::WorkspaceWorkerBuild { .. } => (HUB, on_workspace(old_forms::worker_build)),
        K::RepositoryReach { repo } => {
            (HUB, Before::Record { record: RecordKey::GithubReach { repo: repo.clone() }, says: old_forms::reach })
        }
        K::GithubUsername { .. } => (HUB, on_installation(old_forms::username)),
        K::InstalledOn { .. } => (HUB, on_installation(old_forms::installed_on)),
        K::GithubAppSaid { .. } => (HUB, on_app(old_forms::app_name)),
        K::GithubAppBot { .. } => (HUB, on_app(old_forms::app_bot)),
        K::ModelCatalog { list } => {
            (HUB, Before::Record { record: RecordKey::ModelList(list.clone()), says: old_forms::catalog })
        }
        K::AccountIdentity { account } => {
            (HUB, Before::Record { record: RecordKey::Account { id: account.clone() }, says: old_forms::plan })
        }
        K::AccountLimits { account } => {
            (HUB, Before::Record { record: RecordKey::Account { id: account.clone() }, says: old_forms::limits })
        }
        K::SessionContext { .. } => (SESSION, on_session(old_forms::context)),
        K::SessionTodos { .. } => (SESSION, on_session(old_forms::todos)),
        K::SessionCache { .. } => (HUB, on_session(old_forms::prompt_cache)),
        K::WaitRead { .. } => (SESSION, on_session(old_forms::wait_read)),
        K::ReleaseOffered => (HUB, Before::Record { record: RecordKey::HubUpdate, says: old_forms::release }),
        // Of a kind this build doesn't know, which the hub never makes.
        K::Unknown => return Sent { at: None, before: Before::Itself },
    };
    Sent { at: Some(at), before }
}

fn on_workspace(says: fn(&Record) -> Option<String>) -> Before {
    Before::Record { record: RecordKey::Workspace { id: v() }, says }
}

fn on_installation(says: fn(&Record) -> Option<String>) -> Before {
    Before::Record { record: RecordKey::GithubAccount { id: installation() }, says }
}

fn on_app(says: fn(&Record) -> Option<String>) -> Before {
    Before::Record { record: RecordKey::GithubApp { id: APP }, says }
}

fn on_session(says: fn(&Record) -> Option<String>) -> Before {
    Before::Record { record: RecordKey::Session { id: s() }, says }
}
