//! Each kind of observation, and which one an observation is.

use serde::{Deserialize, Serialize};

use super::{
    AccountIdentity, ConversationCache, Observed, OwnPullRequestDetail, OwnPullRequestSaid, ReferenceSaid, WaitRead,
};
use crate::capability::output::TodoList;
use crate::capability::settings::ModelCatalog;
use crate::model::{
    AccountId, ChangeSummary, ContextUsage, GithubAccountId, GithubUsername, Level, LineCounts, ModelListKey,
    ReferenceAsk, RepositoryUnseen, Rev, SessionId, UsageLimit, WaitId, WorkspaceId,
};

/// One observation, of any kind, with what it's of, its revision and what it says, as a list or the feed gives it. A
/// client keeps whichever copy has the newer revision, as it keeps a record, by [`Observation::key`]. A kind this build
/// doesn't know reads as `Unknown`, which a client doesn't keep.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observation {
    /// How many lines a workspace's changes add and remove against its base, in all. Sent at the hub's level, since a
    /// sidebar draws it for every workspace.
    WorkspaceLines { workspace_id: WorkspaceId, lines: Observed<LineCounts>, rev: Rev },
    /// Every file a workspace changes against its base, with each file's lines and version, as its worker summed them
    /// up. Sent at the workspace's level, since only the workspace's own pane draws it. It's current while something
    /// keeps it up to date, and otherwise as of the last read, which the hub makes when the lines are read.
    WorkspaceChanges { workspace_id: WorkspaceId, changes: Observed<ChangeSummary>, rev: Rev },
    /// How a workspace's pull request stands: which it is, what GitHub last said of it in its summary, and how the
    /// worktree stands against it. Sent at the hub's level, since a sidebar draws every workspace's.
    WorkspacePullRequest { workspace_id: WorkspaceId, pull_request: Observed<OwnPullRequestSaid>, rev: Rev },
    /// What GitHub says of a workspace's own pull request past its summary: its checks, history and stack. Sent at the
    /// workspace's level, since only its pane's bar draws them.
    WorkspacePullRequestDetail { workspace_id: WorkspaceId, detail: Observed<OwnPullRequestDetail>, rev: Rev },
    /// The repository GitHub showed the default GitHub account none of, where the workspace's branch's pull requests
    /// are, as the hub last read them, or `None` where it showed it. Sent at the workspace's level, beside the detail.
    WorkspaceRepositoryUnseen { workspace_id: WorkspaceId, unseen: Observed<Option<RepositoryUnseen>>, rev: Rev },
    /// What GitHub says of an issue or a pull request, as a chip draws it. Sent at the level of that issue or pull
    /// request, by the ask as the client follows it, so two that name it in another case are two observations.
    Reference { reference: ReferenceAsk, said: Observed<ReferenceSaid>, rev: Rev },
    /// What GitHub calls a GitHub account's user now, its username, as the hub last asked GitHub. Sent at the hub's
    /// level, since every view of the account names it. It's current while the hub asks again each day. The user's
    /// number stays on the account's record, as what links the two. A client of a version from before it reads it on
    /// the account's record, as [`ReadFrom::EveryObservation`] says.
    GithubUsername { account: GithubAccountId, username: Observed<GithubUsername>, rev: Rev },
    /// What an agent offers where a chat would launch, as its CLI last listed it, by the list's key, which says the
    /// agent, the project, the provider, the account and the CLI's version. Sent at the hub's level, since the new
    /// workspace dialog and every chat's picker draw it. Never current, since the hub asks again only when it's due.
    /// Whether the hub is asking now, and how that went, is the record [`ModelList`](crate::model::ModelList)'s. A
    /// client of a version from before it reads it on that record, as [`ReadFrom::EveryObservation`] says.
    ModelCatalog { list: ModelListKey, catalog: Observed<ModelCatalog>, rev: Rev },
    /// Who an agent's account is, as a worker last read it from the agent's login, which it does after a launch on a
    /// login on the worker's machine. Sent at the hub's level, since every view of an account names it. Never current,
    /// since nothing tells the hub when it changes. A client of a version from before it reads it on the account's
    /// record, as [`ReadFrom::EveryObservation`] says.
    AccountIdentity { account: AccountId, identity: Observed<AccountIdentity>, rev: Rev },
    /// What's left of an account's usage limits, as its agents last said them, each with when it was heard, shortest
    /// window first and models' weeks by their models. Sent at the hub's level, since a chat's card and Agents in
    /// Settings draw every account's. Never current, since only a running agent says them, and each says only some. A
    /// client of a version from before it reads it on the account's record, as [`ReadFrom::EveryObservation`] says.
    AccountLimits { account: AccountId, limits: Observed<Vec<UsageLimit>>, rev: Rev },
    /// How full a chat's agent's context is, as the agent last said in a report outside the log, of its current run.
    /// Sent at the session's level, since only the chat's pane draws it. Never current, since only the agent says it,
    /// as it works. A client of a version from before it reads it on the session's record, as
    /// [`ReadFrom::EveryObservation`] says.
    SessionContext { session_id: SessionId, context: Observed<ContextUsage>, rev: Rev },
    /// A chat's agent's to-do list, as the agent last said, of its current run. At the session's level, and never
    /// current, as [`Observation::SessionContext`] is.
    SessionTodos { session_id: SessionId, todos: Observed<TodoList>, rev: Rev },
    /// What the API's cache holds of a chat's conversation, and whether its context is big enough that writing it again
    /// costs something. Sent at the hub's level, since the sidebar and Home draw the cache's clock for every chat. It
    /// changes as the hub's record of the chat was announced when it was a field of it: as a request is made while the
    /// agent isn't working, as a turn ends, and as something empties the cache, and not on every request. Never
    /// current.
    SessionCache { session_id: SessionId, cache: Observed<ConversationCache>, rev: Rev },
    /// What an open wait last read of the pull requests it watches, worked out from GitHub's part of the hub's cache as
    /// each read of them ends. Sent at the wait's session's level, since only its chat draws the wait's line. Gone as
    /// the wait ends, when its record takes the last read. Never current. A client of a version from before it reads
    /// it in the wait's `watching`, on the session's record.
    WaitRead { session_id: SessionId, wait: WaitId, read: Observed<WaitRead>, rev: Rev },
    #[serde(other)]
    Unknown,
}

/// Which observation, of which kind, as [`RecordKey`](crate::model::RecordKey) says which record. A kind this build
/// doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationKey {
    WorkspaceLines {
        workspace_id: WorkspaceId,
    },
    WorkspaceChanges {
        workspace_id: WorkspaceId,
    },
    WorkspacePullRequest {
        workspace_id: WorkspaceId,
    },
    WorkspacePullRequestDetail {
        workspace_id: WorkspaceId,
    },
    WorkspaceRepositoryUnseen {
        workspace_id: WorkspaceId,
    },
    Reference {
        reference: ReferenceAsk,
    },
    GithubUsername {
        account: GithubAccountId,
    },
    ModelCatalog {
        list: ModelListKey,
    },
    AccountIdentity {
        account: AccountId,
    },
    AccountLimits {
        account: AccountId,
    },
    SessionContext {
        session_id: SessionId,
    },
    SessionTodos {
        session_id: SessionId,
    },
    SessionCache {
        session_id: SessionId,
    },
    WaitRead {
        session_id: SessionId,
        wait: WaitId,
    },
    #[serde(other)]
    Unknown,
}

impl Observation {
    /// Which one it is. `Unknown` for a kind this build doesn't know.
    pub fn key(&self) -> ObservationKey {
        match self {
            Observation::WorkspaceLines { workspace_id, .. } => {
                ObservationKey::WorkspaceLines { workspace_id: workspace_id.clone() }
            }
            Observation::WorkspaceChanges { workspace_id, .. } => {
                ObservationKey::WorkspaceChanges { workspace_id: workspace_id.clone() }
            }
            Observation::WorkspacePullRequest { workspace_id, .. } => {
                ObservationKey::WorkspacePullRequest { workspace_id: workspace_id.clone() }
            }
            Observation::WorkspacePullRequestDetail { workspace_id, .. } => {
                ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace_id.clone() }
            }
            Observation::WorkspaceRepositoryUnseen { workspace_id, .. } => {
                ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace_id.clone() }
            }
            Observation::Reference { reference, .. } => ObservationKey::Reference { reference: reference.clone() },
            Observation::GithubUsername { account, .. } => ObservationKey::GithubUsername { account: account.clone() },
            Observation::ModelCatalog { list, .. } => ObservationKey::ModelCatalog { list: list.clone() },
            Observation::AccountIdentity { account, .. } => {
                ObservationKey::AccountIdentity { account: account.clone() }
            }
            Observation::AccountLimits { account, .. } => ObservationKey::AccountLimits { account: account.clone() },
            Observation::SessionContext { session_id, .. } => {
                ObservationKey::SessionContext { session_id: session_id.clone() }
            }
            Observation::SessionTodos { session_id, .. } => {
                ObservationKey::SessionTodos { session_id: session_id.clone() }
            }
            Observation::SessionCache { session_id, .. } => {
                ObservationKey::SessionCache { session_id: session_id.clone() }
            }
            Observation::WaitRead { session_id, wait, .. } => {
                ObservationKey::WaitRead { session_id: session_id.clone(), wait: wait.clone() }
            }
            Observation::Unknown => ObservationKey::Unknown,
        }
    }

    /// The revision of this copy, which says whether it replaces another, as [`Rev::supersedes`] does.
    /// `Rev::default()` for a kind this build doesn't know.
    pub fn rev(&self) -> Rev {
        match self {
            Observation::WorkspaceLines { rev, .. }
            | Observation::WorkspaceChanges { rev, .. }
            | Observation::WorkspacePullRequest { rev, .. }
            | Observation::WorkspacePullRequestDetail { rev, .. }
            | Observation::WorkspaceRepositoryUnseen { rev, .. }
            | Observation::Reference { rev, .. }
            | Observation::GithubUsername { rev, .. }
            | Observation::ModelCatalog { rev, .. }
            | Observation::AccountIdentity { rev, .. }
            | Observation::AccountLimits { rev, .. }
            | Observation::SessionContext { rev, .. }
            | Observation::SessionTodos { rev, .. }
            | Observation::SessionCache { rev, .. }
            | Observation::WaitRead { rev, .. } => *rev,
            Observation::Unknown => Rev::default(),
        }
    }
}

impl ObservationKey {
    /// The level the hub sends it at, and so whose subscriptions it reaches. One at the hub's level is listed in
    /// [`Records`](crate::model::Records), which a client reads as it connects. One of another level, such as a
    /// workspace's or a session's, reaches a client only while it follows that, so the hub's list that lacks it doesn't
    /// say it's gone, and only that level's own list does. A kind sent at a workspace's level has a key that names the
    /// workspace, so this can say which workspace's. `None` for one of a kind this build doesn't know, which is of no
    /// level it can follow.
    pub fn level(&self) -> Option<Level> {
        match self {
            ObservationKey::WorkspaceLines { .. }
            | ObservationKey::WorkspacePullRequest { .. }
            | ObservationKey::GithubUsername { .. }
            | ObservationKey::ModelCatalog { .. }
            | ObservationKey::AccountIdentity { .. }
            | ObservationKey::AccountLimits { .. }
            | ObservationKey::SessionCache { .. } => Some(Level::Hub),
            ObservationKey::WorkspaceChanges { workspace_id }
            | ObservationKey::WorkspacePullRequestDetail { workspace_id }
            | ObservationKey::WorkspaceRepositoryUnseen { workspace_id } => {
                Some(Level::Workspace { id: workspace_id.clone() })
            }
            ObservationKey::Reference { reference } => Some(Level::Reference { reference: reference.clone() }),
            ObservationKey::SessionContext { session_id }
            | ObservationKey::SessionTodos { session_id }
            | ObservationKey::WaitRead { session_id, .. } => Some(Level::Session { id: session_id.clone() }),
            ObservationKey::Unknown => None,
        }
    }

    /// Which subscriptions a hub sends it to: each whose client reads it as an observation, rather than in a form from
    /// before it, so no client holds it in two forms. `None` for one of a kind this build doesn't know.
    pub fn read_from(&self) -> Option<ReadFrom> {
        match self {
            ObservationKey::WorkspaceLines { .. } | ObservationKey::WorkspaceChanges { .. } => Some(ReadFrom::Every),
            ObservationKey::WorkspacePullRequest { .. }
            | ObservationKey::WorkspacePullRequestDetail { .. }
            | ObservationKey::WorkspaceRepositoryUnseen { .. }
            | ObservationKey::Reference { .. } => Some(ReadFrom::GithubObserved),
            ObservationKey::GithubUsername { .. }
            | ObservationKey::ModelCatalog { .. }
            | ObservationKey::AccountIdentity { .. }
            | ObservationKey::AccountLimits { .. }
            | ObservationKey::SessionContext { .. }
            | ObservationKey::SessionTodos { .. }
            | ObservationKey::SessionCache { .. }
            | ObservationKey::WaitRead { .. } => Some(ReadFrom::EveryObservation),
            ObservationKey::Unknown => None,
        }
    }

    /// Whether it's of what GitHub says, which a hub sends as an observation only to a start that reads GitHub so, as
    /// [`Subscription::github_observed`](crate::client::subscriptions::Subscription::github_observed) says. `false` for
    /// one of a kind this build doesn't know.
    pub fn is_github(&self) -> bool {
        self.read_from() == Some(ReadFrom::GithubObserved)
    }
}

/// Which subscriptions a hub sends an observation of a kind to, as [`ObservationKey::read_from`] says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadFrom {
    /// Every subscription at its level, since no client reads what it says in another form.
    Every,
    /// A start that says it reads what GitHub says as observations, as
    /// [`Subscription::github_observed`](crate::client::subscriptions::Subscription::github_observed) does, and one of
    /// [`EVERY_OBSERVATION_FROM`](crate::client::EVERY_OBSERVATION_FROM) or newer whatever it says, since a client
    /// from before reads it as `HubFrame::Github`.
    GithubObserved,
    /// A subscription of [`EVERY_OBSERVATION_FROM`](crate::client::EVERY_OBSERVATION_FROM) or newer, since a client
    /// from before reads it in the form it replaced, as a field of a record that the hub fills from it.
    EveryObservation,
}
