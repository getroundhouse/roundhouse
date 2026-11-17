//! What the hub's own tests share.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::Build;
use protocol::capability::Capabilities;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    AgentDescription, AgentFeature, AgentKind, Checks, Effort, Local, LoginDescription, MergeState, ModelChoice,
    ModelName, PermissionMode, PrState, PullRequest, Review, RunId, SessionId, SessionSettings,
};
use protocol::tail::{DeltaChange, StreamDelta, Tail};
use protocol::worker::{PullRequestReply, RateLimit, WorkerRequest, WorkerResponse};
use providers::Provider;
use providers::local::{LocalConfig, LocalProvider};
use tokio::sync::oneshot;

use super::{Hub, HubDirs};
use crate::github::{Github, GithubConfig};
use crate::provider_set::ProviderSet;
use crate::session::{CarriedOver, Item, LiveSession, Starting};
use crate::store::Store;
use crate::store::tests::store_with_session;
use crate::timers::Timers;
use crate::worker_link::WorkerLink;

/// Claude as a hub's tests describe it, which a chat nobody has chosen for runs on Sonnet at High, so a test can tell
/// what the description gave a chat from what anything else would have, and whose token lasts [`A_TOKEN_LASTS_MS`].
/// Its claims say what its lists do: full access and plan mode, an effort, and compacting when asked.
pub(super) fn described() -> AgentDescription {
    AgentDescription {
        capabilities: Some(claimed()),
        kind: AgentKind::Claude,
        name: "Claude".to_string(),
        usage_heading: "Claude usage".to_string(),
        models: vec![ModelChoice { model: ModelName::named("sonnet"), label: "Sonnet".to_string() }],
        efforts: vec![Effort::Low, Effort::High],
        modes: vec![PermissionMode::FullAccess, PermissionMode::Plan],
        defaults: SessionSettings {
            model: ModelName::named("sonnet"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::High,
        },
        features: vec![AgentFeature::Compacts],
        login: Some(LoginDescription {
            made_with: "claude make-a-token".to_string(),
            lasts_ms: Some(A_TOKEN_LASTS_MS),
        }),
    }
}

/// What Claude claims as a hub's tests describe it, as [`described`] says.
pub(crate) fn claimed() -> Capabilities {
    use protocol::capability::approvals::{self, ApprovalMode, Beyond, Modes, Unasked};
    use protocol::capability::{Claimed, context, settings};
    let mode = |plan| ApprovalMode { unasked: Unasked::Everything, beyond: Beyond::Asks, sandbox: None, plan };
    let modes = Modes { default: mode(false), others: vec![mode(true)] };
    let compaction = context::Compaction { keeping: context::Keeping::Read };
    Capabilities {
        approvals: approvals::Claims { modes: Some(modes), ..approvals::Claims::default() },
        settings: settings::Claims { effort: Some(Claimed {}), ..settings::Claims::default() },
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
    let timers = Timers { sign_in_pace: Timers::FOR_TESTS.sign_in_pace, ..Timers::DEFAULT };
    hub_timed(dir, vec![described()], Github::new(GithubConfig::at(api)), timers)
}

/// Where a test's hub that isn't about GitHub has GitHub's API, which nothing serves, so a call that reaches it fails
/// at once.
pub(crate) const NO_GITHUB: &str = "http://127.0.0.1:9";

fn hub_of(dir: &Path, agents: Vec<AgentDescription>, github: Github) -> Arc<Hub> {
    hub_timed(dir, agents, github, Timers::DEFAULT)
}

fn hub_timed(dir: &Path, agents: Vec<AgentDescription>, github: Github, timers: Timers) -> Arc<Hub> {
    let config = LocalConfig {
        worker_exe: dir.join("no-worker"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        shell: Some("/bin/sh".to_string()),
        path: None,
    };
    Arc::new(hub_on(dir, Arc::new(LocalProvider::new(config)), agents, github, timers))
}

fn hub_on(
    dir: &Path,
    provider: Arc<dyn Provider>,
    agents: Vec<AgentDescription>,
    github: Github,
    timers: Timers,
) -> Hub {
    let (store, providers) = (Arc::new(store_with_session(dir)), ProviderSet::only(provider));
    Hub::new(Build::UNSTAMPED, store, HubDirs::in_dir(dir), providers, github, agents, false, timers)
}

/// A hub as [`hub_in`] makes one, before anything shares it, whose workspaces are all `provider`'s.
pub(super) fn hub_provided_by(dir: &Path, provider: Arc<dyn Provider>) -> Hub {
    hub_on(dir, provider, vec![described()], Github::new(GithubConfig::at(NO_GITHUB.to_string())), Timers::DEFAULT)
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
/// `r` is example/project on github.com, and whose workspaces' machines' own `gh`, the default while the hub holds no
/// account, handed it the token `a-token`. It runs with a test's timers, so a test that calls a stand-in, and can't
/// pause tokio's clock, waits out no spacing or pace.
pub(crate) fn hub_reading_github(dir: &Path, api: String) -> Arc<Hub> {
    let hub = hub_timed(dir, vec![described()], Github::new(GithubConfig::at(api)), Timers::FOR_TESTS);
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main", None).unwrap();
    let address = protocol::model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address), None).unwrap();
    let provider = hub.store.get_workspace(&"w".into()).unwrap().provider;
    let token = protocol::worker::FoundToken { token: protocol::model::GithubToken::new("a-token"), renews: false };
    hub.found_tokens.found(provider, Some(token));
    hub
}

/// An open pull request whose checks are running.
pub(super) fn open() -> PullRequest {
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

/// What a stand-in worker says of the pull request when it's read, which a test changes as it goes.
pub(super) type Says = Arc<Mutex<Result<Option<PullRequest>, ApiError>>>;

pub(super) fn says(pull_request: Option<PullRequest>) -> Says {
    Arc::new(Mutex::new(Ok(pull_request)))
}

/// A stand-in for `w`'s worker, as the hub's, that reads its pull request as `says` holds then, and does whatever
/// else it's asked without a word, so it can't say how much of GitHub's limit is left, or how the worktree stands.
pub(super) fn reading(hub: &Hub, says: &Says) -> Arc<WorkerLink> {
    reading_within(hub, says, None)
}

/// The same, which says GitHub's limit stands at `limit`, if it's given one.
pub(super) fn reading_within(hub: &Hub, says: &Says, limit: Option<RateLimit>) -> Arc<WorkerLink> {
    let says = says.clone();
    let link = WorkerLink::answering(move |request| {
        Some(if matches!(request, WorkerRequest::PullRequest(_)) {
            says.lock()
                .clone()
                .map(|pull_request| WorkerResponse::PullRequest(PullRequestReply { pull_request, branch: None }))
        } else if let (WorkerRequest::GithubRateLimit(_), Some(limit)) = (&request, limit) {
            Ok(WorkerResponse::GithubRateLimit(limit))
        } else {
            Ok(WorkerResponse::Ok)
        })
    });
    hub.workers.lock().insert("w".into(), link.clone());
    link
}
