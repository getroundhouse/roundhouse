//! A real hub that calls a stand-in for GitHub, both its API and its device flow, which answers as a test's script
//! says, as the hub's own tests do in `apps/daemon/tests/e2e/github_accounts.rs`.

#![cfg(test)]

use hub::{GithubPullRequests, GithubScript, GithubStandIn};

use super::hub_ahead::HubAhead;
use super::stand_in_gh::StandInGh;
use super::{Stack, git, hub_environment};

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
        let hub = self.start_hub_ahead(hub_environment().set(protocol::paths::ENV_TEST_GITHUB_API, github.api()));
        StandInGithub { _hub: hub, _github: github, _serving: serving }
    }
}

/// The hub, which reads the pull requests of example/project from a stand-in for GitHub that answers from
/// `pull_requests`, as they are when it's asked, with the token the machine's own `gh` hands it. That `gh` is a stand-in
/// too, as [`Stack::start_hub_with_stand_in_gh`] starts one, but logged in. The hub goes first, as with
/// [`StandInGithub`].
pub(crate) struct ReadingStandInGithub {
    _gh: StandInGh,
    pub(crate) pull_requests: GithubPullRequests,
    _github: GithubStandIn,
    _serving: tokio::runtime::Runtime,
}

impl Stack {
    /// Starts the hub before the app opens, reading GitHub from a stand-in, with the repository's `origin` on github.com
    /// as example/project, so the hub knows which repository its pull requests are of.
    pub(crate) fn start_hub_reading_stand_in_github(&self) -> ReadingStandInGithub {
        git(&self.repo(), &["remote", "add", "origin", "https://github.com/example/project.git"]);
        let serving = tokio::runtime::Runtime::new().unwrap();
        let pull_requests = GithubPullRequests::default();
        let script = GithubScript { pull_requests: Some(pull_requests.clone()), ..GithubScript::default() };
        let github = serving.block_on(GithubStandIn::with(script));
        // Logged in as ada, as `gh` 2.103 prints it, with only what's read of it, and the token it uses for that.
        let logged_in = r#"[ "$1 $2" = 'auth status' ] && exec echo '{"hosts":{"github.com":[{"state":"success","active":true,"login":"ada"}]}}'
[ "$1 $2" = 'auth token' ] && exec echo 'gho_made_up_for_ada'"#;
        let mut environment = hub_environment();
        environment.set(protocol::paths::ENV_TEST_GITHUB_API, github.api());
        let gh = self.start_hub_with_gh(&mut environment, logged_in);
        ReadingStandInGithub { _gh: gh, pull_requests, _github: github, _serving: serving }
    }
}
