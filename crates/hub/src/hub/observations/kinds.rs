//! Each kind of observation the store keeps: what it's of, what it says, its table in the cache file and that table's
//! version.

use protocol::capability::output::TodoList;
use protocol::model::{
    AccountId, ChangeSummary, ContextUsage, ConversationCache, LineCounts, ModelListKey, Observation, ObservationKey,
    Observed, OwnPullRequestDetail, OwnPullRequestSaid, RecordKey, ReferenceAsk, ReferenceSaid, RepositoryUnseen, Rev,
    UsageLimit, WorkspaceId,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::kept::Table;
pub(crate) use super::subjects::{Owner, SessionKey, Subject, WaitKey};
use super::tables::Tables;

/// A kind of observation. A new kind is added to [`Observation`] and [`ObservationKey`] in `protocol`, gets a type
/// here, and a table in [`Tables`], whose `each` the compiler stops at.
pub(crate) trait Kind: Sized + 'static {
    /// What it's of, such as a workspace, which its table in the cache file keys it by, in its serde form.
    type Of: Subject;
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

    /// Whether `read` says what `held` says, so a client isn't told of it, and no revision moves: whether the two are
    /// equal, but for a kind that holds what clients aren't told of a change to, such as when each of an account's
    /// limits was heard, which memory and the file keep all the same.
    fn says_the_same(held: &Self::Value, read: &Self::Value) -> bool {
        held == read
    }

    /// The record whose field a client of a version from before this kind reads it in, as
    /// [`OldForm`](super::old_form::OldForm) fills it, whose revision moves as the observation of `of` changes, so such a client
    /// is sent the record again. `None` for a kind no record holds, as one every client reads as an observation, or
    /// as a frame of its own.
    fn old_form_on(_of: &Self::Of) -> Option<RecordKey> {
        None
    }
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

/// What an agent offers where a chat would launch, as its CLI last listed it, sent at the hub's level. A client of a
/// version from before it reads it on the list's record, which the hub keeps in memory, and moves as its lookup does.
pub(crate) struct ModelCatalog;

impl Kind for ModelCatalog {
    type Of = ModelListKey;
    type Value = protocol::capability::settings::ModelCatalog;
    const FORM: Form = Form { table: "model_catalog", version: 1 };

    fn observation(list: &ModelListKey, catalog: Observed<Self::Value>, rev: Rev) -> Observation {
        Observation::ModelCatalog { list: list.clone(), catalog, rev }
    }

    fn key(list: &ModelListKey) -> ObservationKey {
        ObservationKey::ModelCatalog { list: list.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.model_catalog
    }
}

/// Who an agent's account is, as a worker last read it from the agent's login, sent at the hub's level. A client of a
/// version from before it reads it on the account's record.
pub(crate) struct AccountIdentity;

impl Kind for AccountIdentity {
    type Of = AccountId;
    type Value = protocol::model::AccountIdentity;
    const FORM: Form = Form { table: "account_identity", version: 1 };

    fn observation(account: &AccountId, identity: Observed<Self::Value>, rev: Rev) -> Observation {
        Observation::AccountIdentity { account: account.clone(), identity, rev }
    }

    fn key(account: &AccountId) -> ObservationKey {
        ObservationKey::AccountIdentity { account: account.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.account_identity
    }

    fn old_form_on(account: &AccountId) -> Option<RecordKey> {
        Some(RecordKey::Account { id: account.clone() })
    }
}

/// What's left of an account's usage limits, as its agents last said them, each with when it was heard, sent at the
/// hub's level. A client of a version from before it reads it on the account's record. An agent says only some at a
/// time, so each hearing is merged with what's kept, as `crate::hub::accounts` merges it.
pub(crate) struct AccountLimits;

impl Kind for AccountLimits {
    type Of = AccountId;
    type Value = Vec<UsageLimit>;
    const FORM: Form = Form { table: "account_limits", version: 1 };

    fn observation(account: &AccountId, limits: Observed<Self::Value>, rev: Rev) -> Observation {
        Observation::AccountLimits { account: account.clone(), limits, rev }
    }

    fn key(account: &AccountId) -> ObservationKey {
        ObservationKey::AccountLimits { account: account.clone() }
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

    fn old_form_on(account: &AccountId) -> Option<RecordKey> {
        Some(RecordKey::Account { id: account.clone() })
    }
}

/// How full a session's agent's context is, as the agent of its current run last said, sent at the session's level. A
/// client of a version from before it reads it on the session's record.
pub(crate) struct SessionContext;

impl Kind for SessionContext {
    type Of = SessionKey;
    type Value = ContextUsage;
    const FORM: Form = Form { table: "session_context", version: 1 };

    fn observation(of: &SessionKey, context: Observed<ContextUsage>, rev: Rev) -> Observation {
        Observation::SessionContext { session_id: of.session_id.clone(), context, rev }
    }

    fn key(of: &SessionKey) -> ObservationKey {
        ObservationKey::SessionContext { session_id: of.session_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_context
    }

    fn old_form_on(of: &SessionKey) -> Option<RecordKey> {
        Some(RecordKey::Session { id: of.session_id.clone() })
    }
}

/// A session's agent's to-do list, as [`SessionContext`] is.
pub(crate) struct SessionTodos;

impl Kind for SessionTodos {
    type Of = SessionKey;
    type Value = TodoList;
    const FORM: Form = Form { table: "session_todos", version: 1 };

    fn observation(of: &SessionKey, todos: Observed<TodoList>, rev: Rev) -> Observation {
        Observation::SessionTodos { session_id: of.session_id.clone(), todos, rev }
    }

    fn key(of: &SessionKey) -> ObservationKey {
        ObservationKey::SessionTodos { session_id: of.session_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_todos
    }

    fn old_form_on(of: &SessionKey) -> Option<RecordKey> {
        Some(RecordKey::Session { id: of.session_id.clone() })
    }
}

/// What the API's cache holds of a session's conversation, and whether its context is big enough to tell of, sent at
/// the hub's level, as the session worked it out from its agent's requests and its [`SessionContext`]. A client of a
/// version from before it reads what the cache holds on the session's record.
pub(crate) struct SessionCache;

impl Kind for SessionCache {
    type Of = SessionKey;
    type Value = ConversationCache;
    const FORM: Form = Form { table: "session_cache", version: 1 };

    fn observation(of: &SessionKey, cache: Observed<ConversationCache>, rev: Rev) -> Observation {
        Observation::SessionCache { session_id: of.session_id.clone(), cache, rev }
    }

    fn key(of: &SessionKey) -> ObservationKey {
        ObservationKey::SessionCache { session_id: of.session_id.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.session_cache
    }

    fn old_form_on(of: &SessionKey) -> Option<RecordKey> {
        Some(RecordKey::Session { id: of.session_id.clone() })
    }
}

/// What an open wait last read of the pull requests it watches, worked out from GitHub's part of the cache as each read
/// of them ends, sent at its session's level, as `crate::hub::waits` says. A client of a version from before it reads it
/// in the wait's watch, on the session's record.
pub(crate) struct WaitRead;

impl Kind for WaitRead {
    type Of = WaitKey;
    type Value = protocol::model::WaitRead;
    const FORM: Form = Form { table: "wait_read", version: 1 };

    fn observation(of: &WaitKey, read: Observed<Self::Value>, rev: Rev) -> Observation {
        Observation::WaitRead { session_id: of.session_id.clone(), wait: of.wait.clone(), read, rev }
    }

    fn key(of: &WaitKey) -> ObservationKey {
        ObservationKey::WaitRead { session_id: of.session_id.clone(), wait: of.wait.clone() }
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.wait_read
    }

    fn old_form_on(of: &WaitKey) -> Option<RecordKey> {
        Some(RecordKey::Session { id: of.session_id.clone() })
    }
}
