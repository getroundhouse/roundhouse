//! A stand-in for GitHub that the hub reads pull requests from, in the test's own process, so it lasts across the hub
//! starting again, and answers each read from the pull requests of example/project a test sets.

use client_core::Client;
use hub::{GithubPullRequestState, GithubPullRequests, GithubScript, GithubStandIn};
use protocol::model::{Session, Workspace};
use protocol::paths::ENV_TEST_GITHUB_API;

use super::gh::StandInGh;
use super::{Stack, git, wait_until};

pub(crate) struct StandInGithub {
    stand_in: GithubStandIn,
    pull_requests: GithubPullRequests,
    /// What it serves on, which goes with it.
    _runtime: tokio::runtime::Runtime,
}

impl StandInGithub {
    pub(crate) fn start() -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let pull_requests = GithubPullRequests::default();
        let script = GithubScript { pull_requests: Some(pull_requests.clone()), ..GithubScript::default() };
        let stand_in = runtime.block_on(GithubStandIn::with(script));
        Self { stand_in, pull_requests, _runtime: runtime }
    }

    /// Pull request `number` of example/project is `state` from now on.
    pub(crate) fn say(&self, number: u64, state: GithubPullRequestState) {
        self.pull_requests.set(number, state);
    }

    /// Holds every read back until [`StandInGithub::release`].
    pub(crate) fn hold(&self) {
        self.pull_requests.hold();
    }

    pub(crate) fn release(&self) {
        self.pull_requests.release();
    }

    /// A stack whose hub reads GitHub from this, whose repository's `origin` is example/project on github.com, and
    /// whose machine's own `gh` is a stand-in, which hands the hub the token it reads with while that's the default.
    pub(crate) fn stack(&self) -> (Stack, StandInGh) {
        let mut stack = Stack::start_with(vec![(ENV_TEST_GITHUB_API, self.stand_in.api())]);
        git(&stack.repo(), &["remote", "add", "origin", "https://github.com/example/project.git"]);
        let gh = StandInGh::start(&mut stack);
        (stack, gh)
    }
}

/// A workspace of `stack`'s, as [`Stack::workspace`] makes one, once its repository's record has the address its
/// worker read, so the hub knows which repository on GitHub its pull requests are of.
pub(crate) fn workspace_on_github(stack: &Stack, client: &Client) -> (Workspace, Session) {
    let made = stack.workspace(client);
    wait_for_address(client);
    made
}

/// Once a repository's record has the address its worker read.
pub(crate) fn wait_for_address(client: &Client) {
    wait_until("the repository's address", || {
        client.records_list().unwrap().repos.iter().any(|repo| repo.origin.url().is_some())
    });
}
