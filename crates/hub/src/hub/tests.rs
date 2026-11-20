//! What the hub's own tests share.

use std::path::Path;
use std::sync::Arc;

use protocol::Build;
use protocol::capability::Capabilities;
use protocol::capability::settings::Effort;
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    AgentDescription, AgentKind, Apart, Branch, BranchName, Branches, Checks, CommitId, Head, Local, LoginDescription,
    MergeState, PrState, PullRequest, RemoteName, RepoUrl, Review, RunId, SessionId, Upstream,
};
use protocol::tail::{DeltaChange, StreamDelta, Tail};
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use tokio::sync::oneshot;

use super::{Hub, HubDirs};
use crate::github_logins::Connect;
use crate::provider_set::ProviderSet;
use crate::session::{CarriedOver, Item, LiveSession, Starting};
use crate::store::Store;
use crate::store::tests::{open_in, store_with_session};
use crate::timers::Timers;
use crate::worker_link::WorkerLink;
use github::{Github, GithubConfig, PullRequests, Script, StandIn};

/// Claude as a hub's tests describe it, which a chat nobody has chosen for runs on Sonnet at High, so a test can tell
/// what the description gave a chat from what anything else would have, and whose token lasts [`A_TOKEN_LASTS_MS`].
/// Its claims say the rest: Sonnet at Low or High, full access and plan mode, and compacting when asked.
pub(super) fn described() -> AgentDescription {
    AgentDescription {
        capabilities: claimed(),
        kind: AgentKind::Claude,
        name: "Claude".to_string(),
        usage_heading: "Claude usage".to_string(),
        login: Some(LoginDescription {
            made_with: "claude make-a-token".to_string(),
            lasts_ms: Some(A_TOKEN_LASTS_MS),
        }),
    }
}

/// What Claude claims as a hub's tests describe it, as [`described`] says.
pub(crate) fn claimed() -> Capabilities {
    use protocol::capability::approvals;
    use protocol::capability::{Claimed, context, settings};
    let modes = protocol::fixtures::modes();
    let compaction = context::Compaction { keeping: context::Keeping::Read };
    let sonnet = protocol::fixtures::offer("sonnet", "Sonnet", &[Effort::Low, Effort::High], Effort::High);
    let offered = settings::Models::Static(settings::ModelCatalog { offered: vec![sonnet], default_model: None });
    Capabilities {
        approvals: approvals::Claims { modes: Some(modes), ..approvals::Claims::default() },
        settings: settings::Claims { models: Some(offered), effort: Some(Claimed {}), ..settings::Claims::default() },
        context: context::Claims { compaction: Some(compaction), ..context::Claims::default() },
        ..Capabilities::default()
    }
}

/// How long a token lasts as the hub's tests describe Claude, a day, which no real CLI's is.
pub(crate) const A_TOKEN_LASTS_MS: i64 = 24 * 60 * 60 * 1000;

/// A hub whose store has the workspace `w` and its session `s`, with nothing sent in it, and that never starts a
/// worker, so it can't make a workspace, which a hub with
/// [stand-ins for its workers](super::stand_in_workers::hub_with_stand_in_workers) can. Its agents are as
/// [`described`] says.
pub(crate) fn hub_in(dir: &Path) -> Arc<Hub> {
    hub_describing(dir, vec![described()])
}

/// The same, whose agents are as `agents` says.
pub(super) fn hub_describing(dir: &Path, agents: Vec<AgentDescription>) -> Arc<Hub> {
    hub_of(dir, agents, Github::new(GithubConfig::at(NO_GITHUB.to_string())))
}

/// The same, which calls GitHub's API at `api`, such as a stand-in's, and asks after a sign-in's token as often as a
/// test's hub does, as [`Timers::FOR_TESTS`] has it, since a test that calls a stand-in can't pause tokio's clock,
/// which a request to the stand-in would race.
pub(crate) fn hub_calling_github(dir: &Path, api: String) -> Arc<Hub> {
    hub_timed(dir, vec![described()], Github::new(GithubConfig::at(api)), CALLING_GITHUB)
}

/// The same, started again on the store and the files a hub left in `dir`, as a hub that restarts is.
pub(crate) fn hub_calling_github_again(dir: &Path, api: String) -> Arc<Hub> {
    let github = Github::new(GithubConfig::at(api));
    Arc::new(hub_on(open_in(dir), dir, local(dir), vec![described()], github, CALLING_GITHUB))
}

/// How [`hub_calling_github`] times what it does.
const CALLING_GITHUB: Timers = Timers { sign_in_pace: Timers::FOR_TESTS.sign_in_pace, ..Timers::DEFAULT };

/// The same as [`hub_in`], which looks at a worker of another build as often as a test's hub does, as
/// [`Timers::FOR_TESTS`] has it, so a test sees it replaced within a few looks rather than seconds.
pub(crate) fn hub_looking_often(dir: &Path) -> Arc<Hub> {
    let timers = Timers { look_every: Timers::FOR_TESTS.look_every, ..Timers::DEFAULT };
    hub_timed(dir, vec![described()], Github::new(GithubConfig::at(NO_GITHUB.to_string())), timers)
}

/// Where a test's hub that isn't about GitHub has GitHub's API, which nothing serves, so a call that reaches it fails
/// at once.
pub(crate) const NO_GITHUB: &str = "http://127.0.0.1:9";

fn hub_of(dir: &Path, agents: Vec<AgentDescription>, github: Github) -> Arc<Hub> {
    hub_timed(dir, agents, github, Timers::DEFAULT)
}

fn hub_timed(dir: &Path, agents: Vec<AgentDescription>, github: Github, timers: Timers) -> Arc<Hub> {
    Arc::new(hub_on(store_with_session(dir), dir, local(dir), agents, github, timers))
}

/// The local provider, with no worker to start.
fn local(dir: &Path) -> Arc<dyn Provider> {
    let config = LocalConfig {
        worker_exe: dir.join("no-worker"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        path: providers::local::MachinePath::new(Some("/bin/sh".to_string()), None),
    };
    Arc::new(LocalProvider::new(config))
}

fn hub_on(
    store: Store,
    dir: &Path,
    provider: Arc<dyn Provider>,
    agents: Vec<AgentDescription>,
    github: Github,
    timers: Timers,
) -> Hub {
    let (store, providers) = (Arc::new(store), ProviderSet::only(provider));
    // Only `bin` in the test's directory, where a test about `gh` puts a stand-in for it, so no test runs the real one.
    let machine = providers::local::MachinePath::only(Some(dir.join("bin").display().to_string()));
    Hub::new(Build::UNSTAMPED, store, HubDirs::in_dir(dir), providers, github, machine, agents, false, timers)
}

/// A hub as [`hub_describing`] makes one, before anything shares it, whose workspaces are all `provider`'s.
pub(super) fn hub_provided_by(dir: &Path, provider: Arc<dyn Provider>, agents: Vec<AgentDescription>) -> Hub {
    let github = Github::new(GithubConfig::at(NO_GITHUB.to_string()));
    hub_on(store_with_session(dir), dir, provider, agents, github, Timers::DEFAULT)
}

/// A stand-in for `w`'s worker, which fails every call, as `hub`'s worker.
pub(crate) fn stand_in_worker(hub: &Hub) -> Arc<WorkerLink> {
    let link = WorkerLink::answering(|_| Some(Err(ApiError::new(ErrorCode::Worker, "a stand-in does nothing"))));
    serving(hub, &link);
    link
}

/// The hub's store, for a test outside the hub's own module.
pub(crate) fn store_of(hub: &Hub) -> &Arc<Store> {
    &hub.store
}

/// Has `link` be `w`'s worker.
pub(crate) fn serving(hub: &Hub, link: &Arc<WorkerLink>) {
    hub.workers.lock().insert("w".into(), link.clone());
}

/// A stand-in for `w`'s worker that takes everything it's asked, and what it was asked, in order, as JSON.
pub(crate) fn taking_worker(hub: &Hub) -> (Arc<WorkerLink>, Arc<parking_lot::Mutex<Vec<String>>>) {
    let asked = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        kept.lock().push(serde_json::to_string(&request).unwrap());
        Some(Ok(protocol::worker::WorkerResponse::Ok))
    });
    hub.workers.lock().insert("w".into(), link.clone());
    (link, asked)
}

/// Asks to open terminal `id` in `w`, at 80 by 24.
pub(crate) fn terminal_in_w(id: &str) -> protocol::client::terminals::TerminalOpen {
    let size = protocol::terminal::TerminalSize { cols: 80, rows: 24 };
    protocol::client::terminals::TerminalOpen { workspace_id: "w".into(), id: id.into(), size }
}

/// The session `s` of a hub [`hub_in`] made, carried on with from an earlier hub whose agent of `run` is still running
/// on `link`.
pub(crate) fn carried_on(hub: &Arc<Hub>, link: &WorkerLink, run: RunId) -> Arc<LiveSession> {
    let id: SessionId = "s".into();
    let starting = Starting::read(&hub.store, &id).unwrap();
    let carried = CarriedOver { link: link.id, run, recorded: None, untaken: Vec::new() };
    hub.sessions.lock().keep(&id, || LiveSession::start(hub, starting, Some(carried))).unwrap()
}

/// The session `s` started again, as when a hub starts again, from what the store holds, as [`carried_on`] starts it,
/// carrying on with the same agent.
pub(crate) fn started_again(hub: &Arc<Hub>, link: &WorkerLink, run: RunId) -> Arc<LiveSession> {
    hub.sessions.lock().live.remove(&SessionId::from("s"));
    carried_on(hub, link, run)
}

/// Once everything the session has been sent is dealt with.
pub(crate) async fn flushed(live: &LiveSession) {
    let (done, taken) = oneshot::channel();
    live.send_item(Item::Flush(done)).await;
    taken.await.unwrap();
}

/// A worker's tail of `run`, with one message, named for `text`, that has `text` in it.
pub(crate) fn writing(run: RunId, text: &str) -> Tail {
    let mut tail = Tail::default();
    let change = DeltaChange::Thinking { index: 0, text: text.into() };
    let delta = StreamDelta { run: Some(run), message_id: text.into(), subagent: None, change };
    tail.apply(&delta);
    tail
}

/// The same, with `w`'s worktree at `/w`, so what needs it, such as reading its pull request or opening a terminal, can
/// go through a stand-in for its worker.
pub(crate) fn hub_with_worktree(dir: &Path) -> Arc<Hub> {
    let hub = hub_in(dir);
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main", None).unwrap();
    hub
}

/// A hub as [`hub_with_worktree`] makes one, which calls GitHub's API at `api`, such as a stand-in's, whose repository
/// `r` is example/project on github.com, and whose default GitHub account, which it reads GitHub as, is `ada`, whose
/// token is `a-token`. It runs with a test's timers, so a test that calls a stand-in, and can't pause tokio's clock,
/// waits out no spacing or pace.
pub(crate) fn hub_reading_github(dir: &Path, api: String) -> Arc<Hub> {
    hub_reading_github_timed(dir, api, Timers::FOR_TESTS)
}

/// The same, with `timers`.
pub(crate) fn hub_reading_github_timed(dir: &Path, api: String, timers: Timers) -> Arc<Hub> {
    let hub = hub_without_account(dir, api, timers);
    reading_as_ada(&hub);
    hub
}

/// The same, with no GitHub account, so it reads nothing of GitHub.
pub(crate) fn hub_without_account(dir: &Path, api: String, timers: Timers) -> Arc<Hub> {
    let hub = hub_timed(dir, vec![described()], Github::new(GithubConfig::at(api)), timers);
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main", None).unwrap();
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
    hub
}

/// Has the hub hold `ada`'s token, `a-token`, as its first GitHub account, so its default, as connecting it would,
/// without asking GitHub whose it is.
pub(crate) fn reading_as_ada(hub: &Hub) {
    let asked = GithubAccountConnect { token: protocol::model::GithubToken::new("a-token"), id: None };
    let connect = Connect::new(asked, crate::store::now_ms()).unwrap();
    let user = protocol::model::GithubUser { id: protocol::model::GithubUserId::new(7), name: "ada".into() };
    let owner = github::TokenOwner { user, profile: github::GithubProfile::default(), ends_at_ms: None };
    hub.github_logins.connect(&connect, &owner).unwrap();
}

/// An open pull request whose checks are running.
pub(crate) fn open() -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/7".into(),
        title: "T".into(),
        state: PrState::Open,
        is_draft: false,
        base: None,
        merge_state: MergeState::Blocked,
        review: Review::NotRequired,
        checks: Checks { passed: 0, failed: 0, pending: 1 },
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    }
}

/// A stand-in for GitHub answering from the pull requests a test sets, and a hub reading it as [`hub_reading_github`]
/// makes one, with `w` on `branch` at `head-7`, pushed to origin's `branch` at the same commit, as the stand-in's
/// pull requests are from, and #7 is at.
pub(crate) async fn hub_on_github(dir: &Path) -> (Arc<Hub>, PullRequests, StandIn) {
    hub_on_github_timed(dir, Timers::FOR_TESTS).await
}

/// The same, with `timers`.
pub(crate) async fn hub_on_github_timed(dir: &Path, timers: Timers) -> (Arc<Hub>, PullRequests, StandIn) {
    let pull_requests = PullRequests::default();
    let github = StandIn::with(Script { pull_requests: Some(pull_requests.clone()), ..Script::default() }).await;
    let hub = hub_reading_github_timed(dir, github.api(), timers);
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    (hub, pull_requests, github)
}

/// `branch` checked out at `tip`, pushed to origin's `branch`, at `pushed`, `apart` from it as (ahead, behind), on
/// github.com.
pub(crate) fn pushed(tip: &str, pushed: &str, apart: (u32, u32)) -> Branches {
    let name = BranchName::from("branch");
    let upstream = Upstream {
        remote: RemoteName::from("origin"),
        name: name.clone(),
        tip: Some(CommitId::from(pushed)),
        address: Some(RepoUrl::try_from("https://github.com/example/project.git".to_string()).unwrap()),
        apart: Some(Apart { ahead: apart.0, behind: apart.1 }),
    };
    let branch = Branch { name: name.clone(), tip: CommitId::from(tip), upstream: Some(upstream) };
    Branches { head: Head::Branch { name }, had: vec![branch] }
}
