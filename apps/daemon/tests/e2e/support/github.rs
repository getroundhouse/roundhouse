//! A stand-in for GitHub that the hub reads pull requests from, in the test's own process, so it lasts across the hub
//! starting again, and answers each read from the pull requests of example/project a test sets.

use client_core::Client;
use hub::{GithubAnswer, GithubPullRequestState, GithubPullRequests, GithubScript, GithubStandIn};
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::model::{GithubToken, Session, Workspace};
use protocol::paths::ENV_TEST_GITHUB_API;

use super::gh::{LOGGED_IN_AS, LOGGED_IN_WITH, StandInGh};
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
        // As GitHub knows the token a stack's hub is given as its default account's.
        let users = vec![(LOGGED_IN_WITH, GithubAnswer::user(1, LOGGED_IN_AS, None))];
        let script = GithubScript { users, pull_requests: Some(pull_requests.clone()), ..GithubScript::default() };
        let stand_in = runtime.block_on(GithubStandIn::with(script));
        Self { stand_in, pull_requests, _runtime: runtime }
    }

    /// Pull request `number` of example/project is `state` from now on.
    pub(crate) fn say(&self, number: u64, state: GithubPullRequestState) {
        self.pull_requests.set(number, state);
    }

    /// The same, from the branch `branch` of example/project.
    pub(crate) fn say_from(&self, number: u64, state: GithubPullRequestState, branch: &str) {
        self.pull_requests.set_from(number, state, branch);
    }

    /// Every read fails from now on, as GitHub out of reach does, or works again.
    pub(crate) fn fail(&self, failing: bool) {
        self.pull_requests.fail(failing);
    }

    /// Each answer says `remaining` of GitHub's limit of 5000 is left, from now on.
    pub(crate) fn leave(&self, remaining: u32) {
        self.pull_requests.leave(remaining);
    }

    /// How many GraphQL queries it has been sent.
    pub(crate) fn queries(&self) -> usize {
        self.stand_in.asked().iter().filter(|asked| asked.starts_with("post /graphql ")).count()
    }

    /// Holds every read back until [`StandInGithub::release`].
    pub(crate) fn hold(&self) {
        self.pull_requests.hold();
    }

    pub(crate) fn release(&self) {
        self.pull_requests.release();
    }

    /// A stack whose hub reads GitHub from this, as its default account, whose token is [`LOGGED_IN_WITH`], whose
    /// repository's `origin` is example/project on github.com, and whose machine's own `gh` is a stand-in.
    pub(crate) fn stack(&self) -> (Stack, StandInGh) {
        let mut stack = Stack::start_with(vec![(ENV_TEST_GITHUB_API, self.stand_in.api())]);
        git(&stack.repo(), &["remote", "add", "origin", "https://github.com/example/project.git"]);
        let gh = StandInGh::start(&mut stack);
        let connect = GithubAccountConnect { token: GithubToken::new(LOGGED_IN_WITH), id: None };
        stack.client().call(connect).unwrap();
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

/// The same, with its branch pushed to `origin`, as far as its worktree knows, at the commit it's at, as a push would
/// leave it, once the hub's record says so, as its worker tells it, and once the read of its pull request that the
/// push brings has found what GitHub has from the branch, so it can't land after a test's own.
pub(crate) fn pushed_workspace(stack: &Stack, client: &Client) -> (Workspace, Session) {
    let (workspace, session) = workspace_on_github(stack, client);
    push(client, &workspace);
    wait_until("the read the push brings", || super::workspace(client, &workspace.id).pull_request_link.is_some());
    (workspace, session)
}

/// Has `workspace`'s branch pushed to `origin`, as [`pushed_workspace`] says.
pub(crate) fn push(client: &Client, workspace: &Workspace) {
    let path = std::path::PathBuf::from(workspace.path.as_ref().unwrap());
    let branch = workspace.branch.as_str();
    git(&path, &["update-ref", &format!("refs/remotes/origin/{branch}"), "HEAD"]);
    git(&path, &["branch", "-q", "--set-upstream-to", &format!("origin/{branch}")]);
    wait_until("the branch to be pushed, as its record says", || {
        let branches = super::workspace(client, &workspace.id).branches;
        branches.as_ref().and_then(|branches| branches.checked_out()).is_some_and(|branch| branch.upstream.is_some())
    });
}

/// Once a repository's record has the address its worker read.
pub(crate) fn wait_for_address(client: &Client) {
    wait_until("the repository's address", || {
        client.records().unwrap().repos.iter().any(|repo| repo.origin.url().is_some())
    });
}
