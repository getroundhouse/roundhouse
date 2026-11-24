//! Made-up observations, one builder for each kind a test says, each at revision 1, as in a hub made in one change.
//! What only an agent says is as of [`SAID_AT_MS`], since a hub can't ask for it again, and what the hub watches or asks
//! for again is current, as a hub that's keeping it up to date sends it.

use crate::capability::output::TodoList;
use crate::capability::settings::ModelCatalog;
use crate::model::{
    AccountId, AccountIdentity, Branches, ContextUsage, ConversationCache, GithubAccountId, GithubAppId, GithubAppSaid,
    GithubUser, GithubUsername, ModelListKey, Observation, Observed, OtherBuild, Reach, ReleaseOffer, RepoId, Rev,
    SessionId, UsageLimit, WaitId, WaitRead, WorkspaceId,
};

/// When what an agent says was said, in a test that doesn't say: 1 November 2026, at 3 PM in UTC, which is when the
/// fake hub's pictures are drawn.
pub const SAID_AT_MS: i64 = 1_793_545_200_000;

/// The revision of each.
const REV: Rev = Rev::new(1);

/// `value`, as kept up to date now.
pub fn current<T>(value: T) -> Observed<T> {
    Observed::Current { value }
}

/// `value`, as of [`SAID_AT_MS`].
pub fn said<T>(value: T) -> Observed<T> {
    Observed::AsOf { value, as_of_ms: SAID_AT_MS }
}

/// Who the account is, as an agent under it said.
pub fn identity(account: impl Into<AccountId>, identity: AccountIdentity) -> Observation {
    Observation::AccountIdentity { account: account.into(), identity: said(identity), rev: REV }
}

/// What's left of the account's limits, as an agent under it said.
pub fn limits(account: impl Into<AccountId>, limits: Vec<UsageLimit>) -> Observation {
    Observation::AccountLimits { account: account.into(), limits: said(limits), rev: REV }
}

/// What GitHub calls the GitHub account's user.
pub fn username(account: impl Into<GithubAccountId>, name: &str) -> Observation {
    Observation::GithubUsername { account: account.into(), username: current(GithubUsername::from(name)), rev: REV }
}

/// What GitHub calls the user or organization the GitHub account, an installation of an app of your own, is on.
pub fn installed_on(account: impl Into<GithubAccountId>, name: &str) -> Observation {
    Observation::InstalledOn { account: account.into(), on: current(GithubUsername::from(name)), rev: REV }
}

/// What GitHub says of the app of your own `app`.
pub fn app_said(app: u64, said: GithubAppSaid) -> Observation {
    Observation::GithubAppSaid { app: GithubAppId::new(app), said: current(said), rev: REV }
}

/// The app of your own `app`'s bot, as GitHub numbers it.
pub fn app_bot(app: u64, bot: GithubUser) -> Observation {
    Observation::GithubAppBot { app: GithubAppId::new(app), bot: current(bot), rev: REV }
}

/// What a push from the repository `repo`'s workspaces would meet on GitHub.
pub fn reach(repo: impl Into<RepoId>, reach: Reach) -> Observation {
    Observation::RepositoryReach { repo: repo.into(), reach: current(reach), rev: REV }
}

/// What an agent that discovers its own models offers where `list` says, as it answered.
pub fn catalog(list: ModelListKey, catalog: ModelCatalog) -> Observation {
    Observation::ModelCatalog { list, catalog: said(catalog), rev: REV }
}

/// The release the update feed offers, `None` where none is newer than the hub.
pub fn release_offered(version: Option<&str>) -> Observation {
    let offered = ReleaseOffer { version: version.map(str::to_string) };
    Observation::ReleaseOffered { offered: current(offered), rev: REV }
}

/// The workspace's branches.
pub fn branches(workspace: impl Into<WorkspaceId>, branches: Branches) -> Observation {
    Observation::WorkspaceBranches { workspace_id: workspace.into(), branches: current(branches), rev: REV }
}

/// The workspace's worker, which is the build `build` rather than the hub's own, or one too old to say which, if
/// `None`.
pub fn worker_build(workspace: impl Into<WorkspaceId>, build: Option<&str>) -> Observation {
    let build = OtherBuild { said: build.map(str::to_string) };
    Observation::WorkspaceWorkerBuild { workspace_id: workspace.into(), build: current(build), rev: REV }
}

/// How full the chat's agent's context is, as it said.
pub fn context(session: impl Into<SessionId>, context: ContextUsage) -> Observation {
    Observation::SessionContext { session_id: session.into(), context: said(context), rev: REV }
}

/// The chat's agent's to-do list, as it said.
pub fn todos(session: impl Into<SessionId>, todos: TodoList) -> Observation {
    Observation::SessionTodos { session_id: session.into(), todos: said(todos), rev: REV }
}

/// What the API's cache holds of the chat's conversation, as its agent's newest request left it.
pub fn cache(session: impl Into<SessionId>, cache: ConversationCache) -> Observation {
    Observation::SessionCache { session_id: session.into(), cache: said(cache), rev: REV }
}

/// What the chat's open wait `wait` last read.
pub fn wait_read(session: impl Into<SessionId>, wait: impl Into<WaitId>, read: WaitRead) -> Observation {
    Observation::WaitRead { session_id: session.into(), wait: wait.into(), read: said(read), rev: REV }
}
