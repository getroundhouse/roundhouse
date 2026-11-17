//! What the worker's unit tests share.

use process::Environment;

/// What a test gives the worker's own git and `gh`, as the product gives them what its hub set: the whole environment
/// that a test's own git runs in, [`process::testing::git_environment`], in place of the test's own, so the product's
/// git reads none of the person's config, hooks or credentials, nor a `GIT_DIR` from a hook they're in, and a test of
/// it does the same whoever runs it. A test adds to it what the product would, such as the git credential's config,
/// and never starts from an environment that adds nothing, which leaves the product's git all of the test's own, as
/// `tools/policy` checks.
pub(crate) fn given() -> Environment {
    process::testing::git_environment().given()
}
