//! How many migrations there were before some of them, counting the floor's, so a test can make an older database and
//! see what migrating keeps.

/// How many migrations there were before each workspace recorded its settled repository source and worker environment.
pub(crate) const BEFORE_WORKSPACE_PLACEMENT: usize = 80;

/// How many migrations there were before the hub recorded the providers whose machines Roundhouse leaves GitHub to.
pub(crate) const BEFORE_GITHUB_LEFT_TO_MACHINE: usize = 84;

/// How many migrations there were before the hub recorded the providers Roundhouse sets up GitHub on, and forgot each
/// provider's found `gh` login.
pub(crate) const BEFORE_GITHUB_SET_UP: usize = 85;

/// How many migrations there were before disconnecting a GitHub account removed it, but for an app's installation.
pub(crate) const BEFORE_GITHUB_ACCOUNT_REMOVAL: usize = 86;

/// How many migrations there were before each workspace's record kept only which pull request is its own.
pub(crate) const BEFORE_PULL_REQUEST_LINKS: usize = 90;

/// How many migrations there were before each wait's watch kept the pull requests it saw as their facets.
pub(crate) const BEFORE_WAITS_AS_FACETS: usize = 92;

/// How many migrations there were before who each account is left the database, as an observation.
pub(crate) const BEFORE_ACCOUNT_IDENTITY_OBSERVED: usize = 93;

/// How many migrations there were before what's left of each account's limits left the database, as an observation.
pub(crate) const BEFORE_ACCOUNT_LIMITS_OBSERVED: usize = 94;

/// How many migrations there were before how full each session's agent's context is, and its to-do list, left the
/// database, as observations.
pub(crate) const BEFORE_SESSION_CONTEXT_OBSERVED: usize = 95;

/// How many migrations there were before what the API's cache holds of each session's conversation left the database,
/// as an observation.
pub(crate) const BEFORE_SESSION_CACHE_OBSERVED: usize = 96;

/// How many migrations there were before what each open wait last read left the database, as an observation.
pub(crate) const BEFORE_WAIT_READ_OBSERVED: usize = 97;

/// How many migrations there were before what GitHub calls each GitHub account's user, and its profile, left the
/// database, as observations.
pub(crate) const BEFORE_GITHUB_USERS_OBSERVED: usize = 98;

/// How many migrations there were before whether the default GitHub account can push to each repository left the
/// database, as an observation.
pub(crate) const BEFORE_REACH_OBSERVED: usize = 99;

/// How many migrations there were before what GitHub says of each app of your own, and where each installation is,
/// left the database, as observations.
pub(crate) const BEFORE_APPS_OBSERVED: usize = 100;

/// How many migrations there were before each workspace's branches, and each of its other worktrees', left the
/// database, as observations.
pub(crate) const BEFORE_BRANCHES_OBSERVED: usize = 101;

/// How many migrations there were before which build each workspace's worker is left the database, as an observation.
pub(crate) const BEFORE_WORKER_BUILD_OBSERVED: usize = 102;
