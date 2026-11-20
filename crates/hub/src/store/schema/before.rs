//! How many migrations there were before some of them, so a test can make an older database and see what migrating
//! keeps.

/// How many migrations there were before events recorded their run, so a test can make an older database.
pub(crate) const BEFORE_RUNS: usize = 4;

/// How many migrations there were before workspaces had titles.
pub(crate) const BEFORE_TITLES: usize = 9;

/// How many migrations there were before sessions had an effort.
pub(crate) const BEFORE_EFFORT: usize = 13;

/// How many migrations there were before workspaces kept their pull request.
pub(crate) const BEFORE_PULL_REQUESTS: usize = 14;

/// How many migrations there were before sessions kept how full their agent's context is.
pub(crate) const BEFORE_CONTEXT: usize = 15;

/// How many migrations there were before the hub kept settings of its own.
pub(crate) const BEFORE_HUB_SETTINGS: usize = 17;

/// How many migrations there were before every session named its model and effort.
pub(crate) const BEFORE_CHOSEN_MODELS: usize = 18;

/// How many migrations there were before accounts were records.
pub(crate) const BEFORE_ACCOUNTS: usize = 19;

/// How many migrations there were before chats closed unused were deleted.
pub(crate) const BEFORE_UNUSED_DELETED: usize = 20;

/// How many migrations there were before events recorded their subagent.
pub(crate) const BEFORE_SUBAGENTS: usize = 21;

/// How many migrations there were before subagents had records.
pub(crate) const BEFORE_SUBAGENT_RECORDS: usize = 22;

/// How many migrations there were before every agent had full access.
pub(crate) const BEFORE_FULL_ACCESS: usize = 23;

/// How many migrations there were before subagents' records kept what each has done lately.
pub(crate) const BEFORE_RECENT_WORK: usize = 24;

/// How many migrations there were before sessions kept when the person last sent their agent something.
pub(crate) const BEFORE_USER_ACTIVITY: usize = 25;

/// How many migrations there were before workspaces kept when the person last did something in them.
pub(crate) const BEFORE_WORKSPACE_ACTIVITY: usize = 26;

/// How many migrations there were before the hub recorded each instance it asks a provider for.
pub(crate) const BEFORE_INSTANCES: usize = 31;

/// How many migrations there were before subagents' records kept the model each runs on.
pub(crate) const BEFORE_SUBAGENT_MODELS: usize = 32;

/// How many migrations there were before sessions' records said what their agents wait on.
pub(crate) const BEFORE_WAITING_ON: usize = 41;

/// How many migrations there were before the new-workspace dialog's tables were named for it.
pub(crate) const BEFORE_NEW_WORKSPACE_DRAFT_NAMES: usize = 43;

/// How many migrations there were before subagents' records kept how many tokens each one's conversation holds.
pub(crate) const BEFORE_SUBAGENT_CONTEXT_TOKENS: usize = 44;
/// How many migrations there were before the hub held logins for agents' accounts.
pub(crate) const BEFORE_LOGINS: usize = 45;

/// How many migrations there were before sessions' records said how their newest turn ended.
pub(crate) const BEFORE_REPLY_OUTCOME: usize = 47;

/// How many migrations there were before the hub could allow a tool's use itself when an agent with full access asks.
pub(crate) const BEFORE_ALLOWING_ASKED: usize = 49;

/// How many migrations there were before each model's own week of an account's limits was kept apart.
pub(crate) const BEFORE_LIMIT_MODELS: usize = 50;

/// How many migrations there were before accounts kept when they were last used.
pub(crate) const BEFORE_ACCOUNTS_USED: usize = 51;

/// How many migrations there were before a session's conversation ID could be one its agent hadn't reported yet.
pub(crate) const BEFORE_AGENT_RUNS: usize = 53;

/// Before per-agent account defaults were durable hub settings.
pub(crate) const BEFORE_ACCOUNT_DEFAULTS: usize = 54;

/// How many migrations there were before chats said where their login comes from, in place of the account they were on.
pub(crate) const BEFORE_ACCOUNT_SOURCES: usize = 56;

/// How many migrations there were before each subagent had an ID apart from the call that started it.
pub(crate) const BEFORE_SUBAGENT_IDS: usize = 57;

/// How many migrations there were before the hub held GitHub accounts.
pub(crate) const BEFORE_GITHUB_ACCOUNTS: usize = 58;

/// How many migrations there were before the hub signed in with GitHub.
pub(crate) const BEFORE_GITHUB_SIGN_INS: usize = 59;

/// How many migrations there were before the hub kept apps of your own.
pub(crate) const BEFORE_GITHUB_APPS: usize = 62;

/// How many migrations there were before the hub recorded whether the default GitHub account can push to each
/// repository.
pub(crate) const BEFORE_GITHUB_REACH: usize = 63;

/// How many migrations there were before the hub kept what GitHub's profile of each GitHub account's user says.
pub(crate) const BEFORE_GITHUB_PROFILES: usize = 64;

/// How many migrations there were before the hub kept each repository's address.
pub(crate) const BEFORE_REPO_URLS: usize = 65;

/// How many migrations there were before the hub recorded what it sends every worker for the default GitHub account.
pub(crate) const BEFORE_GITHUB_SENT: usize = 66;

/// How many migrations there were before the hub kept whether a push from each repository's checkout goes round the
/// git credential.
pub(crate) const BEFORE_PUSH_ROUTES: usize = 67;

/// How many migrations there were before Claude's calls were typed.
pub(crate) const BEFORE_TYPED_CALLS: usize = 69;

/// How many migrations there were before requests for approval, and what closed them, were typed.
pub(crate) const BEFORE_TYPED_REQUESTS: usize = 70;

/// How many migrations there were before each chat's approval mode, and each its log names, were typed.
pub(crate) const BEFORE_TYPED_MODES: usize = 71;

/// How many migrations there were before each workspace recorded its settled repository source and worker environment.
pub(crate) const BEFORE_WORKSPACE_PLACEMENT: usize = 80;

/// How many migrations there were before the hub recorded the providers whose machines Roundhouse leaves GitHub to.
pub(crate) const BEFORE_GITHUB_LEFT_TO_MACHINE: usize = 84;

/// How many migrations there were before the hub recorded the providers Roundhouse sets up GitHub on, and forgot each
/// provider's found `gh` login.
pub(crate) const BEFORE_GITHUB_SET_UP: usize = 85;

/// How many migrations there were before disconnecting a GitHub account removed it, but for an app's installation.
pub(crate) const BEFORE_GITHUB_ACCOUNT_REMOVAL: usize = 86;
