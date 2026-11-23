//! Each kind of observation the store keeps: what it's of, what it says, its table in the cache file and that table's
//! version.

use std::hash::Hash;

use protocol::model::{
    ChangeSummary, GithubRepositoryId, GithubRepositorySaid, LineCounts, Observation, ObservationKey, Observed,
    OwnPullRequestDetail, OwnPullRequestSaid, ReferenceAsk, ReferenceSaid, RepositoryUnseen, Rev, WorkspaceId,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::kept::{Table, Tables};

/// A kind of observation. A new kind is added to [`Observation`] and [`ObservationKey`] in `protocol`, gets a type
/// here, and a table in [`Tables`], whose `each` the compiler stops at.
pub(crate) trait Kind: Sized + 'static {
    /// What it's of, such as a workspace, which its table in the cache file keys it by, in its serde form.
    type Of: Clone + Eq + Hash + Serialize + DeserializeOwned + Send + Sync;
    /// What it says, which its table in the cache file keeps in its serde form.
    type Value: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync;
    /// Its table in the cache file, and that table's version. Raise the version with any change to the serde form of
    /// [`Kind::Of`] or [`Kind::Value`], and a hub of the build that does empties this kind alone, rather than
    /// migrating it.
    const FORM: Form;

    /// It, as a client is sent it.
    fn observation(of: &Self::Of, said: Observed<Self::Value>, rev: Rev) -> Observation;

    /// Which it is, as its removal names it.
    fn key(of: &Self::Of) -> ObservationKey;

    /// Its table among the store's.
    fn table(tables: &mut Tables) -> &mut Table<Self>;
}

/// A kind's table in the cache file, and the version of its form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Form {
    pub(crate) table: &'static str,
    pub(crate) version: i64,
}

/// How many lines a workspace's changes add and remove against its base, in all.
pub(crate) struct WorkspaceLines;

impl Kind for WorkspaceLines {
    type Of = WorkspaceId;
    type Value = LineCounts;
    const FORM: Form = Form { table: "workspace_lines", version: 1 };

    fn observation(workspace_id: &WorkspaceId, lines: Observed<LineCounts>, rev: Rev) -> Observation {
        Observation::WorkspaceLines { workspace_id: workspace_id.clone(), lines, rev }
    }

    fn key(workspace_id: &WorkspaceId) -> ObservationKey {
        ObservationKey::WorkspaceLines { workspace_id: workspace_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_lines
    }
}

/// Every file a workspace changes against its base, with each file's lines and version, as its worker summed them up,
/// sent at the workspace's level.
pub(crate) struct WorkspaceChanges;

impl Kind for WorkspaceChanges {
    type Of = WorkspaceId;
    type Value = ChangeSummary;
    const FORM: Form = Form { table: "workspace_changes", version: 1 };

    fn observation(workspace_id: &WorkspaceId, changes: Observed<ChangeSummary>, rev: Rev) -> Observation {
        Observation::WorkspaceChanges { workspace_id: workspace_id.clone(), changes, rev }
    }

    fn key(workspace_id: &WorkspaceId) -> ObservationKey {
        ObservationKey::WorkspaceChanges { workspace_id: workspace_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_changes
    }
}

/// What GitHub says of an issue or a pull request a chip names, by the ask as the client follows it. Worked out from
/// what GitHub's cache keeps of it, as `crate::hub::github_observed` says.
pub(crate) struct Referenced;

impl Kind for Referenced {
    type Of = ReferenceAsk;
    type Value = ReferenceSaid;
    const FORM: Form = Form { table: "referenced", version: 1 };

    fn observation(reference: &ReferenceAsk, said: Observed<ReferenceSaid>, rev: Rev) -> Observation {
        Observation::Reference { reference: reference.clone(), said, rev }
    }

    fn key(reference: &ReferenceAsk) -> ObservationKey {
        ObservationKey::Reference { reference: reference.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.referenced
    }
}

/// How a workspace's pull request stands, with its summary and how the worktree stands against it, sent at the hub's
/// level. Worked out from GitHub's cache and the workspace's record, as `crate::hub::github_observed` says.
pub(crate) struct WorkspacePullRequest;

impl Kind for WorkspacePullRequest {
    type Of = WorkspaceId;
    type Value = OwnPullRequestSaid;
    const FORM: Form = Form { table: "workspace_pull_request", version: 1 };

    fn observation(workspace_id: &WorkspaceId, pull_request: Observed<OwnPullRequestSaid>, rev: Rev) -> Observation {
        Observation::WorkspacePullRequest { workspace_id: workspace_id.clone(), pull_request, rev }
    }

    fn key(workspace_id: &WorkspaceId) -> ObservationKey {
        ObservationKey::WorkspacePullRequest { workspace_id: workspace_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_pull_request
    }
}

/// A workspace's pull request's checks, history and stack, of the head they were read at, sent at the workspace's
/// level. Worked out as [`WorkspacePullRequest`] is.
pub(crate) struct WorkspacePullRequestDetail;

impl Kind for WorkspacePullRequestDetail {
    type Of = WorkspaceId;
    type Value = OwnPullRequestDetail;
    const FORM: Form = Form { table: "workspace_pull_request_detail", version: 1 };

    fn observation(workspace_id: &WorkspaceId, detail: Observed<OwnPullRequestDetail>, rev: Rev) -> Observation {
        Observation::WorkspacePullRequestDetail { workspace_id: workspace_id.clone(), detail, rev }
    }

    fn key(workspace_id: &WorkspaceId) -> ObservationKey {
        ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_pull_request_detail
    }
}

/// Whether GitHub showed the default GitHub account no repository where a workspace's branch's pull requests are, sent
/// at the workspace's level. Worked out from what the hub last heard of it.
pub(crate) struct WorkspaceRepositoryUnseen;

impl Kind for WorkspaceRepositoryUnseen {
    type Of = WorkspaceId;
    type Value = Option<RepositoryUnseen>;
    const FORM: Form = Form { table: "workspace_repository_unseen", version: 1 };

    fn observation(workspace_id: &WorkspaceId, unseen: Observed<Option<RepositoryUnseen>>, rev: Rev) -> Observation {
        Observation::WorkspaceRepositoryUnseen { workspace_id: workspace_id.clone(), unseen, rev }
    }

    fn key(workspace_id: &WorkspaceId) -> ObservationKey {
        ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_repository_unseen
    }
}

/// What GitHub says of a repository a project is linked to, by GitHub's ID for it, as `crate::hub::repo_links` reads
/// it.
pub(crate) struct GithubRepositories;

impl Kind for GithubRepositories {
    type Of = GithubRepositoryId;
    type Value = GithubRepositorySaid;
    const FORM: Form = Form { table: "github_repositories", version: 1 };

    fn observation(id: &GithubRepositoryId, said: Observed<GithubRepositorySaid>, rev: Rev) -> Observation {
        Observation::GithubRepository { id: id.clone(), said, rev }
    }

    fn key(id: &GithubRepositoryId) -> ObservationKey {
        ObservationKey::GithubRepository { id: id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_repositories
    }
}
