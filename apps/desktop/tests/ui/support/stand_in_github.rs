//! A real hub that calls a stand-in for GitHub, both its API and its device flow, which answers as a test's script
//! says, as the hub's own tests do in `apps/daemon/tests/e2e/github_accounts.rs`.

#![cfg(test)]

use hub::{GithubScript, GithubStandIn};

use super::Stack;
use super::hub_ahead::HubAhead;

/// The hub, and the stand-in it calls, which serves until the test ends. The hub is stopped first, as the fields are
/// dropped in order, so it never calls a stand-in that has gone.
pub(crate) struct StandInGithub {
    _hub: HubAhead,
    _github: GithubStandIn,
    _serving: tokio::runtime::Runtime,
}

impl Stack {
    /// Starts the hub before the app opens, calling a stand-in for GitHub that answers as `script` says.
    pub(crate) fn start_hub_with_stand_in_github(&self, script: GithubScript) -> StandInGithub {
        let serving = tokio::runtime::Runtime::new().unwrap();
        let github = serving.block_on(GithubStandIn::with(script));
        let hub = self.start_hub_ahead(&[(protocol::paths::ENV_TEST_GITHUB_API, github.api())]);
        StandInGithub { _hub: hub, _github: github, _serving: serving }
    }
}
