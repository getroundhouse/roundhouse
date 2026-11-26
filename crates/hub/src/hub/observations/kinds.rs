//! Each kind of observation the store keeps but GitHub's of the hub's own accounts and repositories: what it's of, what
//! it says, its table in the cache file and that table's version, as [`Kind`] has a kind say them.

use protocol::capability::output::TodoList;
use protocol::capability::setup::ToolServerState;
use protocol::model::{
    AccountId, ChangeSummary, ContextUsage, ConversationCache, LineCounts, ModelListKey, Observation, ObservationKey,
    Observed, OwnPullRequestDetail, OwnPullRequestSaid, ReferenceAsk, ReferenceSaid, RepositoryUnseen, Rev, UsageLimit,
    WorkspaceId,
};

use super::keepers::Unkept;
use super::kept::Table;
pub(crate) use super::kind::{Form, Kind};
pub(crate) use super::subjects::{Owner, SessionKey, Subject, WaitKey};
use super::tables::Tables;

/// How many lines a workspace's changes add and remove against its base, in all.
pub(crate) struct WorkspaceLines;

impl Kind for WorkspaceLines {
    type Of = WorkspaceId;
    type Value = LineCounts;
    const FORM: Form = Form { table: "workspace_lines", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfThen;

    fn observation(workspace_id: &WorkspaceId, lines: Observed<LineCounts>, rev: Rev) -> Option<Observation> {
        Some(Observation::WorkspaceLines { workspace_id: workspace_id.clone(), lines, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspaceLines { workspace_id: workspace_id.clone() })
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
    const UNKEPT: Unkept = Unkept::AsOfThen;

    fn observation(workspace_id: &WorkspaceId, changes: Observed<ChangeSummary>, rev: Rev) -> Option<Observation> {
        Some(Observation::WorkspaceChanges { workspace_id: workspace_id.clone(), changes, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspaceChanges { workspace_id: workspace_id.clone() })
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
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(reference: &ReferenceAsk, said: Observed<ReferenceSaid>, rev: Rev) -> Option<Observation> {
        Some(Observation::Reference { reference: reference.clone(), said, rev })
    }

    fn key(reference: &ReferenceAsk) -> Option<ObservationKey> {
        Some(ObservationKey::Reference { reference: reference.clone() })
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
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(
        workspace_id: &WorkspaceId,
        pull_request: Observed<OwnPullRequestSaid>,
        rev: Rev,
    ) -> Option<Observation> {
        Some(Observation::WorkspacePullRequest { workspace_id: workspace_id.clone(), pull_request, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspacePullRequest { workspace_id: workspace_id.clone() })
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
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(
        workspace_id: &WorkspaceId,
        detail: Observed<OwnPullRequestDetail>,
        rev: Rev,
    ) -> Option<Observation> {
        Some(Observation::WorkspacePullRequestDetail { workspace_id: workspace_id.clone(), detail, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace_id.clone() })
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
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(
        workspace_id: &WorkspaceId,
        unseen: Observed<Option<RepositoryUnseen>>,
        rev: Rev,
    ) -> Option<Observation> {
        Some(Observation::WorkspaceRepositoryUnseen { workspace_id: workspace_id.clone(), unseen, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_repository_unseen
    }
}

/// What an agent offers where a chat would launch, as its CLI last listed it, sent at the hub's level.
pub(crate) struct ModelCatalog;

impl Kind for ModelCatalog {
    type Of = ModelListKey;
    type Value = protocol::capability::settings::ModelCatalog;
    const FORM: Form = Form { table: "model_catalog", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(list: &ModelListKey, catalog: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::ModelCatalog { list: list.clone(), catalog, rev })
    }

    fn key(list: &ModelListKey) -> Option<ObservationKey> {
        Some(ObservationKey::ModelCatalog { list: list.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.model_catalog
    }
}

/// Who an agent's account is, as a worker last read it from the agent's login, sent at the hub's level.
pub(crate) struct AccountIdentity;

impl Kind for AccountIdentity {
    type Of = AccountId;
    type Value = protocol::model::AccountIdentity;
    const FORM: Form = Form { table: "account_identity", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(account: &AccountId, identity: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::AccountIdentity { account: account.clone(), identity, rev })
    }

    fn key(account: &AccountId) -> Option<ObservationKey> {
        Some(ObservationKey::AccountIdentity { account: account.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.account_identity
    }
}

/// What's left of an account's usage limits, as its agents last said them, each with when it was heard, sent at the
/// hub's level. An agent says only some at a time, so each hearing is merged with what's kept, as
/// `crate::hub::accounts` merges it.
pub(crate) struct AccountLimits;

impl Kind for AccountLimits {
    type Of = AccountId;
    type Value = Vec<UsageLimit>;
    const FORM: Form = Form { table: "account_limits", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(account: &AccountId, limits: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::AccountLimits { account: account.clone(), limits, rev })
    }

    fn key(account: &AccountId) -> Option<ObservationKey> {
        Some(ObservationKey::AccountLimits { account: account.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.account_limits
    }

    /// When each was heard, which hearing the same again changes, isn't what a client is told of, since it would be
    /// told on every hearing of limits that haven't moved.
    fn says_the_same(held: &Self::Value, read: &Self::Value) -> bool {
        let without_when = |limits: &[UsageLimit]| -> Vec<UsageLimit> {
            limits.iter().map(|limit| UsageLimit { heard_at_ms: None, ..limit.clone() }).collect()
        };
        without_when(held) == without_when(read)
    }
}

/// How full a session's agent's context is, as the agent of its current run last said, sent at the session's level.
pub(crate) struct SessionContext;

impl Kind for SessionContext {
    type Of = SessionKey;
    type Value = ContextUsage;
    const FORM: Form = Form { table: "session_context", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(of: &SessionKey, context: Observed<ContextUsage>, rev: Rev) -> Option<Observation> {
        Some(Observation::SessionContext { session_id: of.session_id.clone(), context, rev })
    }

    fn key(of: &SessionKey) -> Option<ObservationKey> {
        Some(ObservationKey::SessionContext { session_id: of.session_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_context
    }
}

/// A session's agent's to-do list, as [`SessionContext`] is.
pub(crate) struct SessionTodos;

impl Kind for SessionTodos {
    type Of = SessionKey;
    type Value = TodoList;
    const FORM: Form = Form { table: "session_todos", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(of: &SessionKey, todos: Observed<TodoList>, rev: Rev) -> Option<Observation> {
        Some(Observation::SessionTodos { session_id: of.session_id.clone(), todos, rev })
    }

    fn key(of: &SessionKey) -> Option<ObservationKey> {
        Some(ObservationKey::SessionTodos { session_id: of.session_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_todos
    }
}

/// The tool servers a session's agent has, as the agent of its current run last said, sent at the session's level, and
/// gone as that run ends and as another starts, as `crate::session` sees to.
pub(crate) struct SessionToolServers;

impl Kind for SessionToolServers {
    type Of = SessionKey;
    type Value = Vec<ToolServerState>;
    const FORM: Form = Form { table: "session_tool_servers", version: 1 };
    const UNKEPT: Unkept = Unkept::Gone;

    fn observation(of: &SessionKey, servers: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::SessionToolServers { session_id: of.session_id.clone(), servers, rev })
    }

    fn key(of: &SessionKey) -> Option<ObservationKey> {
        Some(ObservationKey::SessionToolServers { session_id: of.session_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_tool_servers
    }
}

/// What the API's cache holds of a session's conversation, and whether its context is big enough to tell of, sent at
/// the hub's level, as the session worked it out from its agent's requests and its [`SessionContext`].
pub(crate) struct SessionCache;

impl Kind for SessionCache {
    type Of = SessionKey;
    type Value = ConversationCache;
    const FORM: Form = Form { table: "session_cache", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(of: &SessionKey, cache: Observed<ConversationCache>, rev: Rev) -> Option<Observation> {
        Some(Observation::SessionCache { session_id: of.session_id.clone(), cache, rev })
    }

    fn key(of: &SessionKey) -> Option<ObservationKey> {
        Some(ObservationKey::SessionCache { session_id: of.session_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_cache
    }
}

/// What an open wait last read of the pull requests it watches, worked out from GitHub's part of the cache as each read
/// of them ends, sent at its session's level, as `crate::hub::waits` says.
pub(crate) struct WaitRead;

impl Kind for WaitRead {
    type Of = WaitKey;
    type Value = protocol::model::WaitRead;
    const FORM: Form = Form { table: "wait_read", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(of: &WaitKey, read: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::WaitRead { session_id: of.session_id.clone(), wait: of.wait.clone(), read, rev })
    }

    fn key(of: &WaitKey) -> Option<ObservationKey> {
        Some(ObservationKey::WaitRead { session_id: of.session_id.clone(), wait: of.wait.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.wait_read
    }
}
