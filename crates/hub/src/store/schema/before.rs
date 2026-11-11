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
