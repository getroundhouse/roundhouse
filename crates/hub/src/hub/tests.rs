//! What the hub's own tests share.

use std::path::Path;
use std::sync::Arc;
use tracing::Instrument as _;

use protocol::Build;
use protocol::capability::Capabilities;
use protocol::capability::settings::Effort;
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::{ApiError, ErrorCode, HubStart};
use protocol::model::{
    AgentDescription, AgentKind, Apart, Branch, BranchName, Branches, CommitId, Head, LoginDescription, RemoteName,
    RepoUrl, RunId, SessionId, Upstream,
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
use github::{Github, GithubConfig, PullRequests, References, Script, StandIn};

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

/// The same, started again on the store and the files a hub left in `dir`, as a hub that restarts is.
pub(crate) fn hub_in_again(dir: &Path) -> Arc<Hub> {
    let github = Github::new(GithubConfig::at(NO_GITHUB.to_string()));
    Arc::new(hub_on(open_in(dir), dir, local(dir), vec![described()], github, Timers::DEFAULT, &[]))
}

/// The same, which calls GitHub's API at `api`, such as a stand-in's, and asks after a sign-in's token as often as a
/// test's hub does, as [`Timers::FOR_TESTS`] has it, since a test that calls a stand-in can't pause tokio's clock,
/// which a request to the stand-in would race.
pub(crate) fn hub_calling_github(dir: &Path, api: String) -> Arc<Hub> {
    hub_timed(dir, vec![described()], Github::new(GithubConfig::at(api)), CALLING_GITHUB)
}

/// The same, whose machine's environment, as the profile would give it, has `given` too.
pub(crate) fn hub_calling_github_given(dir: &Path, api: String, given: &[(&str, &str)]) -> Arc<Hub> {
    let github = Github::new(GithubConfig::at(api));
    Arc::new(hub_on(store_with_session(dir), dir, local(dir), vec![described()], github, CALLING_GITHUB, given))
}

/// The same, started again on the store and the files a hub left in `dir`, as a hub that restarts is.
pub(crate) fn hub_calling_github_again(dir: &Path, api: String) -> Arc<Hub> {
    let github = Github::new(GithubConfig::at(api));
    Arc::new(hub_on(open_in(dir), dir, local(dir), vec![described()], github, CALLING_GITHUB, &[]))
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
    Arc::new(hub_on(store_with_session(dir), dir, local(dir), agents, github, timers, &[]))
}

/// The local provider, with no worker to start.
fn local(dir: &Path) -> Arc<dyn Provider> {
    let config = LocalConfig {
        worker_exe: dir.join("no-worker"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
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
    given: &[(&str, &str)],
) -> Hub {
    let (store, providers) = (Arc::new(store), ProviderSet::only(provider));
    // Only `bin` in the test's directory, where a test about `gh` puts a stand-in for it, so no test runs the real one.
    let path = dir.join("bin").display().to_string();
    let vars: Vec<(&str, &str)> = [("PATH", path.as_str())].into_iter().chain(given.iter().copied()).collect();
    let machine = crate::machine_environment::MachineEnvironment::only(&vars);
    Hub::new(
        Build::UNSTAMPED,
        store,
        HubDirs::in_dir(dir),
        providers,
        github,
        machine,
        agents,
        HubStart::ByHand,
        timers,
    )
}

/// A hub as [`hub_describing`] makes one, before anything shares it, whose workspaces are all `provider`'s.
pub(super) fn hub_provided_by(dir: &Path, provider: Arc<dyn Provider>, agents: Vec<AgentDescription>) -> Hub {
    let github = Github::new(GithubConfig::at(NO_GITHUB.to_string()));
    hub_on(store_with_session(dir), dir, provider, agents, github, Timers::DEFAULT, &[])
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

/// What a stand-in worker answers a request its test isn't about: its tools served, on the port asked for or else on
/// 4100, as every launch asks first, and anything else done.
pub(crate) fn taken(request: &protocol::worker::WorkerRequest) -> protocol::worker::WorkerResponse {
    use protocol::worker::{ServeTools, ToolsServing, WorkerRequest, WorkerResponse};
    if let WorkerRequest::ServeTools(ServeTools { port }) = request {
        return WorkerResponse::ToolsServing(ToolsServing { port: port.unwrap_or(4100) });
    }
    WorkerResponse::Ok
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
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main").unwrap();
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
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main").unwrap();
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
    hub
}

/// Has the hub hold `ada`'s token, `a-token`, as its first GitHub account, so its default, as connecting it would,
/// without asking GitHub whose it is, and know who she is on GitHub.
pub(crate) fn reading_as_ada(hub: &Hub) {
    let asked = GithubAccountConnect { token: protocol::model::GithubToken::new("a-token"), id: None };
    let connect = Connect::new(asked, crate::store::now_ms()).unwrap();
    let user = protocol::model::GithubUser { id: protocol::model::GithubUserId::new(7), name: "ada".into() };
    let owner = github::TokenOwner { user, profile: github::GithubProfile::default(), ends_at_ms: None };
    let account = hub.github_logins.connect(&connect, &owner).unwrap().account.id;
    crate::github_logins::tests::heard(&hub.github_logins, &account, &owner, crate::store::now_ms());
}

/// A stand-in for GitHub answering from the pull requests a test sets, and a hub reading it as [`hub_reading_github`]
/// makes one, with `w` on `branch` at `head-7`, pushed to origin's `branch` at the same commit, as the stand-in's
/// pull requests are from, and #7 is at.
pub(crate) async fn hub_on_github(dir: &Path) -> (Arc<Hub>, PullRequests, StandIn) {
    hub_on_github_timed(dir, Timers::FOR_TESTS).await
}

/// The same, with `timers`.
pub(crate) async fn hub_on_github_timed(dir: &Path, timers: Timers) -> (Arc<Hub>, PullRequests, StandIn) {
    hub_on_github_answering(dir, timers, None).await
}

/// The same, where GitHub answers for `references` too, as its issues and pull requests by their numbers.
pub(crate) async fn hub_on_github_with(dir: &Path, references: &References) -> (Arc<Hub>, PullRequests, StandIn) {
    hub_on_github_answering(dir, Timers::FOR_TESTS, Some(references.clone())).await
}

async fn hub_on_github_answering(
    dir: &Path,
    timers: Timers,
    references: Option<References>,
) -> (Arc<Hub>, PullRequests, StandIn) {
    let pull_requests = PullRequests::default();
    let github =
        StandIn::with(Script { pull_requests: Some(pull_requests.clone()), references, ..Script::default() }).await;
    let hub = hub_reading_github_timed(dir, github.api(), timers);
    hub.branches_known(&"w".into(), pushed("head-7", "head-7", (0, 0))).await;
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

/// Records the workspace `w` as archived, as archiving it does once it's done, with nothing else archiving does.
pub(crate) fn archived(hub: &Hub) {
    use crate::store::{Ended, Move};
    hub.store.start_move(&"w".into(), Move::Archive).unwrap().unwrap();
    hub.store.end_move(&"w".into(), Move::Archive, Ended::Done).unwrap();
}

/// Has the hub's cache forget all it has of GitHub, as it does what's too old, or as after its file was deleted.
pub(crate) fn forget_github(hub: &Hub) {
    hub.github_cache.age(std::time::Duration::from_hours(24 * 8));
    hub.github_cache.keep(Vec::new(), crate::store::now_ms());
}

/// The same, but for the summary of `w`'s pull request, which the hub has read, as a chip's read of it keeps it then.
pub(crate) fn forget_github_but_a_summary(hub: &Hub) {
    let workspace = hub.store.get_workspace(&"w".into()).unwrap();
    let own = hub.known_of(&workspace).and_then(|found| found.pull_request).expect("w's pull request, read");
    forget_github(hub);
    let summary = protocol::model::PullRequest { history: None, stack: None, checks: None, ..own.pull_request };
    let number = protocol::model::IssueNumber::try_from(summary.number.get()).unwrap();
    let item = crate::github_cache::Item::new(&summary.repository, number);
    let kept = protocol::model::Reference::PullRequest(summary);
    hub.github_cache.keep(vec![(item, Ok(Some(kept)))], crate::store::now_ms());
}

/// Has the hub's cache keep `reference`, which a chip asks for as `ask`, as read now, as a read of it would.
pub(crate) fn github_kept(hub: &Hub, ask: &protocol::model::ReferenceAsk, reference: protocol::model::Reference) {
    let item = crate::github_cache::Item::new(&ask.repository, ask.number);
    hub.github_cache.keep(vec![(item, Ok(Some(reference)))], crate::store::now_ms());
}

/// A summary of a worktree whose one changed file, `a.rs`, adds `added` lines and removes `removed`.
pub(crate) fn changes(added: u64, removed: u64) -> protocol::model::ChangeSummary {
    use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, LineCounts};

    let lines = Some(LineCounts { added, removed });
    let file = ChangedFile {
        path: "a.rs".into(),
        from: None,
        status: ChangeStatus::Modified,
        lines,
        version: None,
        name_not_utf8: false,
    };
    ChangeSummary::new("base".into(), vec![file])
}

/// On a thread of its own, which has none of the spans of the task it's for unless it's given them.
#[tokio::test]
async fn what_runs_off_the_async_threads_is_in_the_spans_its_asked_for_in() {
    let _logs = logging::testing::Logs::capture();
    let named = || Ok::<_, ApiError>(tracing::Span::current().metadata().map(tracing::Metadata::name));

    let within = super::coordinator::off_thread(named).instrument(crate::spans::request(1, 2)).await.unwrap();

    assert_eq!(within, Some("request"));
}
