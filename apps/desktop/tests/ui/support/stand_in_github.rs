//! A real hub that calls a stand-in for GitHub, both its API and its device flow, which answers as a test's script
//! says, as the hub's own tests do in `apps/daemon/tests/e2e/github_accounts.rs`.

#![cfg(test)]

use hub::{GithubAnswer, GithubPullRequests, GithubScript, GithubStandIn};
use protocol::model::{GithubToken, WorkspaceId};

use super::hub_ahead::HubAhead;
use super::stand_in_gh::StandInGh;
use super::{Stack, Ui, git, hub_environment};

/// The token the hub reading a stand-in for GitHub is given, as its default account's, `ada`'s, which `gh` there uses
/// too.
const TOKEN: &str = "gho_made_up_for_ada";

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
/// `pull_requests`, as they are when it's asked, as its default GitHub account, `ada`, whose token it was given. The
/// machine's own `gh` is a stand-in too, logged in as `ada`, so nothing an agent asks `gh` reaches GitHub. The hub goes
/// first, as with [`StandInGithub`].
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
        // As GitHub knows the token the hub is given as its default account's.
        let users = vec![(TOKEN, GithubAnswer::user(1, "ada", None))];
        let script = GithubScript { users, pull_requests: Some(pull_requests.clone()), ..GithubScript::default() };
        let github = serving.block_on(GithubStandIn::with(script));
        // Logged in as ada, as `gh` 2.103 prints it, with only what's read of it, and the token it uses for that.
        let logged_in = r#"[ "$1 $2" = 'auth status' ] && exec echo '{"hosts":{"github.com":[{"state":"success","active":true,"login":"ada"}]}}'
[ "$1 $2" = 'auth token' ] && exec echo 'gho_made_up_for_ada'"#;
        let mut environment = hub_environment();
        environment.set(protocol::paths::ENV_TEST_GITHUB_API, github.api());
        let gh = self.start_hub_with_gh(&mut environment, logged_in);
        self.client().github_account_connect(GithubToken::new(TOKEN), None).unwrap();
        ReadingStandInGithub { _gh: gh, pull_requests, _github: github, _serving: serving }
    }
}

impl Ui<'_> {
    /// Has workspace `id`'s branch pushed to `origin`, as far as its worktree knows, at the commit it's at, as a push
    /// would leave it, once the hub's record says so, and once the read of its pull request the push brings has found
    /// what GitHub has from it, so that read can't land after a test's own. Returns the branch, and the commit it's at.
    pub(crate) fn push(&mut self, id: &WorkspaceId) -> (String, String) {
        let record = |ui: &Self| ui.view().records().workspaces().iter().find(|w| &w.id == id).cloned().unwrap();
        let workspace = record(self);
        let (path, branch) = (std::path::PathBuf::from(workspace.path.unwrap()), workspace.branch.to_string());
        git(&path, &["update-ref", &format!("refs/remotes/origin/{branch}"), "HEAD"]);
        git(&path, &["branch", "-q", "--set-upstream-to", &format!("origin/{branch}")]);
        self.wait("the push to be read", |ui| {
            let workspace = record(ui);
            let pushed = workspace.branches.as_ref().and_then(|branches| branches.checked_out());
            pushed.is_some_and(|branch| branch.upstream.is_some()) && workspace.pull_request.is_some()
        });
        let head = git(&path, &["rev-parse", "HEAD"]).trim().to_string();
        (branch, head)
    }
}
