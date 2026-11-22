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
