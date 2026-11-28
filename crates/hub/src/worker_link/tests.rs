//! Starting a worker, against the local provider with a script in place of the worker.

use std::os::unix::process::CommandExt;
use std::path::Path;

use process::ProcessRecord;
use protocol::client::HubStart;
use protocol::client::tool_servers::ToolServerAdd;
use protocol::frame;
use protocol::model::{self, GitFile, ToolServerAuthGiven};
use protocol::paths::WORKER_RECORD_FILE;
use protocol::worker::{
    AskId, AskOutcome, AskReply, CancelSetup, GitCredentialSet, GitStatus, GithubCredential, PullRequestAnswer,
    ToolServersSet, WorkerAsk, WorkerCall, WorkerOutcome, WorkerRequest,
};
use providers::local::{LocalConfig, LocalProvider};

use super::*;
use crate::hub::HubDirs;
use crate::hub::tests::{NO_GITHUB, pushed, reading_as_ada, serving};
use crate::provider_set::ProviderSet;
use crate::store::tests::store_with_session;
use crate::timers::Timers;
use github::{Github, PullRequestState, PullRequests, Script, StandIn};

/// A local provider whose worker is `script`.
fn provider_running(dir: &Path, script: &str) -> Arc<dyn Provider> {
    let exe = dir.join("worker");
    process::testing::write_program(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: exe,
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        worker_logs_dir: dir.join("worker-logs"),
        environment: process::Environment::NONE,
        dials: false,
        hub_address_file: None,
    }))
}

/// A hub whose store has the workspace `w`, ready at `/w`, and its session `s`, and whose workers `provider` starts.
/// Its repository's address has been read, so a worker isn't asked for it, which would come between the calls these
/// tests spell out, in no order of their own.
fn hub_using(dir: &Path, provider: &Arc<dyn Provider>) -> Arc<Hub> {
    hub_timed(dir, provider, Timers::DEFAULT)
}

/// A hub as [`hub_using`] makes one, which reads pull requests from GitHub's API at `api`, whose repository is
/// example/project on github.com, and whose workspace `w` is on `branch`, pushed there, as a stand-in's pull requests
/// are from.
fn hub_reading(dir: &Path, provider: &Arc<dyn Provider>, api: String) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main").unwrap();
    let address = model::RepoUrl::try_from("https://github.com/example/project".to_string()).unwrap();
    store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
    let providers = ProviderSet::only(provider.clone());
    let github = Github::new(github::GithubConfig::at(api));
    let (dirs, timers) = (HubDirs::in_dir(dir), Timers::FOR_TESTS);
    let hub = Arc::new(Hub::new(
        Build::UNSTAMPED,
        store,
        dirs,
        providers,
        github,
        crate::machine_environment::MachineEnvironment::only(&[]),
        Vec::new(),
        HubStart::ByHand,
        timers,
    ));
    hub.keep_branches_as_read(&"w".into(), pushed("head-7", "head-7", (0, 0)), 1_000);
    reading_as_ada(&hub);
    hub
}

/// The same, with `timers`.
fn hub_timed(dir: &Path, provider: &Arc<dyn Provider>, timers: Timers) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main").unwrap();
    store.read_repo_url(&"r".into(), "/repo", None).unwrap();
    let providers = ProviderSet::only(provider.clone());
    let github = Github::new(github::GithubConfig::at(NO_GITHUB.to_string()));
    Arc::new(Hub::new(
        Build::UNSTAMPED,
        store,
        HubDirs::in_dir(dir),
        providers,
        github,
        crate::machine_environment::MachineEnvironment::only(&[]),
        Vec::new(),
        HubStart::ByHand,
        timers,
    ))
}

/// Writes each frame to a file in `dir` by its name, for a script to send.
fn frames_in(dir: &Path, frames: &[(&str, WorkerFrame)]) {
    for (name, frame) in frames {
        std::fs::write(dir.join(name), frame::encode(frame).unwrap()).unwrap();
    }
}

#[tokio::test]
async fn a_worker_refused_its_instance_leaves_the_one_serving_it_running() {
    let dir = tempfile::tempdir().unwrap();
    let refusal = WorkerFrame::Reply {
        id: 1,
        outcome: WorkerOutcome::Error(ApiError::new(ErrorCode::Conflict, "another worker is serving it")),
    };
    frames_in(dir.path(), &[("refusal", refusal)]);
    let provider = provider_running(
        dir.path(),
        &format!("read call\ncat '{}'\ncat >/dev/null", dir.path().join("refusal").display()),
    );
    let instance_id: InstanceId = "w".into();
    provider.create(&instance_id, None, None).await.unwrap();
    let mut serving = std::process::Command::new("sleep").arg("100").process_group(0).spawn().unwrap();
    ProcessRecord::write(&dir.path().join("instances/i").join(WORKER_RECORD_FILE), serving.id()).unwrap();
    let hub = hub_using(dir.path(), &provider);

    let opened =
        WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log"), Opening::Started)
            .await;

    assert!(matches!(&opened, Err(e) if e.code == ErrorCode::Conflict), "{:?}", opened.err());
    assert!(serving.try_wait().unwrap().is_none(), "the worker serving the instance was ended");
    serving.kill().unwrap();
    serving.wait().unwrap();
}

/// As from a newer worker, after a rollback.
#[tokio::test]
async fn a_reply_this_hub_cant_read_fails_its_call_rather_than_leaving_it_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let reply = r#"{"frame":"reply","id":1,"response":{"result":"from_a_newer_worker"}}"#;
    let provider = provider_running(dir.path(), &format!("read call\necho '{reply}'\ncat >/dev/null"));
    let instance_id: InstanceId = "w".into();
    provider.create(&instance_id, None, None).await.unwrap();
    // A time limit far longer than the test waits, so a call that waits for it rather than failing fails the test.
    let hub = hub_timed(dir.path(), &provider, Timers { hello_within: Duration::from_mins(10), ..Timers::DEFAULT });

    let log = dir.path().join("logs/i.log");
    let opening = WorkerLink::open(hub, provider, "w".into(), instance_id, &log, Opening::Started);
    let opened = tokio::time::timeout(Duration::from_secs(10), opening).await.expect("it waited for the time limit");

    let Err(e) = opened else { panic!("the hello's reply was read") };
    assert_eq!(e.code, ErrorCode::Worker, "{e:?}");
}

/// A call reads its reply as what its request is answered with, so one of another kind fails it here, rather than in
/// each caller.
#[tokio::test]
async fn a_reply_of_another_kind_than_its_request_is_answered_with_fails_its_call() {
    let link = WorkerLink::answering(|_| Some(Ok(WorkerResponse::Ok)));

    let status = link.call(GitStatus { path: "/w".into() }).await;
    let cancelled = link.call(CancelSetup).await;

    assert_eq!(status.map_err(|e| e.code), Err(ErrorCode::Worker));
    assert_eq!(cancelled, Ok(()));
}

/// One that chose a version this hub doesn't speak, which no worker should, since a worker chooses one both speak.
#[tokio::test]
async fn a_worker_that_chose_another_version_is_not_used() {
    let dir = tempfile::tempdir().unwrap();
    let reply = r#"{"frame":"reply","id":1,"response":{"result":"hello","protocol_version":9999}}"#;
    let provider = provider_running(dir.path(), &format!("read call\necho '{reply}'\ncat >/dev/null"));
    let instance_id: InstanceId = "w".into();
    provider.create(&instance_id, None, None).await.unwrap();
    let hub = hub_using(dir.path(), &provider);

    let opened =
        WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log"), Opening::Started)
            .await;

    let Err(e) = opened else { panic!("it was used") };
    assert_eq!(e.code, ErrorCode::VersionMismatch, "{e:?}");
}

/// The hub reads it from GitHub, then asks the worker that asked, which is running, how many files are uncommitted,
/// so its answer waits on the worker answering that.
#[tokio::test]
async fn a_worker_asking_for_its_pull_request_is_answered_once_it_has_answered_the_calls_that_takes() {
    let dir = tempfile::tempdir().unwrap();
    let pull_requests = PullRequests::default();
    pull_requests.set(7, PullRequestState::Open);
    let github = StandIn::with(Script { pull_requests: Some(pull_requests), ..Script::default() }).await;
    let file = GitFile { path: "a".into(), orig_path: None, status: ".M".into() };
    let status = WorkerResponse::GitStatus(model::GitStatus { files: vec![file], ..model::GitStatus::default() });
    frames_in(
        dir.path(),
        &[
            ("ask", WorkerFrame::Ask { id: AskId::new(9), session_id: "s".into(), ask: WorkerAsk::PullRequest }),
            ("status", WorkerFrame::Reply { id: 4, outcome: WorkerOutcome::Response(status) }),
        ],
    );
    let d = dir.path().display();
    // Once it's the workspace's worker, which is the one a read asks.
    let script = format!(
        "{}while [ ! -e '{d}/go' ]; do sleep 0.05; done\ncat '{d}/ask'\nread -r call\ncat '{d}/status'\n{}",
        greeted(dir.path(), WorkerOutcome::Response(WorkerResponse::Ok)),
        answer_to_file(dir.path())
    );
    let provider = provider_running(dir.path(), &script);
    provider.create(&"w".into(), None, None).await.unwrap();
    let hub = hub_reading(dir.path(), &provider, github.api());

    let link = WorkerLink::open(
        hub.clone(),
        provider,
        "w".into(),
        "w".into(),
        &dir.path().join("logs/i.log"),
        Opening::Started,
    )
    .await
    .unwrap();
    serving(&hub, &link);
    std::fs::write(dir.path().join("go"), "").unwrap();

    let answer = answered(dir.path()).await;
    let AskOutcome::Response(AskReply::PullRequest(PullRequestAnswer { pull_request: Some(pr) })) = answer.outcome
    else {
        panic!("not the pull request: {answer:?}")
    };
    assert_eq!((answer.answer, pr.pull_request.number.get(), pr.local.uncommitted), (AskId::new(9), 7, 1));
    link.kill();
}

/// As from a newer worker, after a rollback, so the agent's call is refused at once rather than left for the worker
/// to give up on.
#[tokio::test]
async fn an_ask_this_hub_cant_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ask = r#"{"frame":"ask","id":9,"session_id":{"newer":true},"ask":{"method":"pull_request"}}"#;
    let greeted = greeted(dir.path(), WorkerOutcome::Response(WorkerResponse::Ok));
    let script = format!("{greeted}echo '{ask}'\n{}", answer_to_file(dir.path()));
    let provider = provider_running(dir.path(), &script);
    provider.create(&"w".into(), None, None).await.unwrap();
    let hub = hub_using(dir.path(), &provider);

    let link =
        WorkerLink::open(hub, provider, "w".into(), "w".into(), &dir.path().join("logs/i.log"), Opening::Started)
            .await
            .unwrap();

    let answer = answered(dir.path()).await;
    assert_eq!(answer.answer, AskId::new(9));
    assert!(matches!(&answer.outcome, AskOutcome::Error(e) if e.code == ErrorCode::Unsupported), "{answer:?}");
    link.kill();
}

/// The worker answers the hello only once it has read both sets too, so a hub that waited for the hello's answer before
/// it queued them would fail, and the hub's next call, as a launch would be, comes after them, with nothing between.
#[tokio::test]
async fn a_worker_is_set_the_hubs_git_credential_and_tool_servers_right_behind_its_hello_before_anything_else() {
    let dir = tempfile::tempdir().unwrap();

    let (link, credential) = greeted_link(dir.path(), WorkerOutcome::Response(WorkerResponse::Ok)).await;
    let cancelled = link.call(CancelSetup).await;

    let call = |name: &str| frame::decode::<WorkerCall>(&std::fs::read(dir.path().join(name)).unwrap()).unwrap();
    let set = WorkerRequest::GitCredentialSet(GitCredentialSet { credential });
    assert_eq!(call("set"), WorkerCall { id: 2, request: set });
    let servers = call("servers");
    let WorkerCall { id: 3, request: WorkerRequest::ToolServersSet(ToolServersSet { servers }) } = servers else {
        panic!("not the tool servers: {servers:?}")
    };
    assert_eq!(servers.iter().map(|server| server.label.as_str()).collect::<Vec<_>>(), ["Docs"]);
    assert_eq!(call("next"), WorkerCall { id: 4, request: WorkerRequest::CancelSetup });
    assert_eq!(cancelled, Ok(()));
    link.kill();
}

/// A link to a worker that answers the hub's set of its git credential with `set_answer`, and the call after it, which
/// it keeps in `next` in `dir`, as done, and what the hub had to set as it opened.
async fn greeted_link(dir: &Path, set_answer: WorkerOutcome) -> (Arc<WorkerLink>, GithubCredential) {
    let done = WorkerFrame::Reply { id: 4, outcome: WorkerOutcome::Response(WorkerResponse::Ok) };
    frames_in(dir, &[("done", done)]);
    let d = dir.display();
    let next = format!("read -r next\nprintf '%s\\n' \"$next\" > '{d}/next'\ncat '{d}/done'\ncat >/dev/null");
    let provider = provider_running(dir, &format!("{}{next}", greeted(dir, set_answer)));
    provider.create(&"w".into(), None, None).await.unwrap();
    // Short, so a hub that waits for the hello's answer before it sets the credential fails soon.
    let hub = hub_timed(dir, &provider, Timers { hello_within: Duration::from_secs(2), ..Timers::DEFAULT });
    let docs = ToolServerAdd {
        id: "t".into(),
        name: "Docs".into(),
        url: "https://mcp.example.com/mcp".into(),
        auth: ToolServerAuthGiven::Neither,
    };
    hub.tool_server_add(docs).await.unwrap();
    let credential = hub.git_credential().borrow().for_workers();
    let log = dir.join("logs/i.log");
    let opening = WorkerLink::open(hub, provider, "w".into(), "w".into(), &log, Opening::Started);
    let opened = tokio::time::timeout(Duration::from_secs(5), opening).await.expect("the link to open");
    (opened.unwrap(), credential)
}

/// For a script: reads the hub's hello and the git credential and tool servers it sets behind it, keeping the sets in
/// `set` and `servers` in `dir`, then answers the hello, the credential with `set`, and the servers as done, as its
/// first three calls.
fn greeted(dir: &Path, set: WorkerOutcome) -> String {
    let hello = WorkerResponse::Hello(HelloReply { protocol_version: versions::WORKERS.newest, build: None });
    frames_in(
        dir,
        &[
            ("hello", WorkerFrame::Reply { id: 1, outcome: WorkerOutcome::Response(hello) }),
            ("set-answer", WorkerFrame::Reply { id: 2, outcome: set }),
            ("servers-answer", WorkerFrame::Reply { id: 3, outcome: WorkerOutcome::Response(WorkerResponse::Ok) }),
        ],
    );
    let d = dir.display();
    let keep = |name: &str| format!("read -r {name}\nprintf '%s\\n' \"${name}\" > '{d}/{name}'\n");
    format!(
        "read -r call\n{}{}cat '{d}/hello'\ncat '{d}/set-answer'\ncat '{d}/servers-answer'\n",
        keep("set"),
        keep("servers")
    )
}

/// For a script: reads the hub's next line, an answer, into `answered` in `dir`, all at once, then waits.
fn answer_to_file(dir: &Path) -> String {
    let d = dir.display();
    format!(
        "read -r answer\nprintf '%s\\n' \"$answer\" > '{d}/answered.tmp'\nmv '{d}/answered.tmp' '{d}/answered'\ncat >/dev/null"
    )
}

/// The answer a script with [`answer_to_file`] in `dir` was given.
async fn answered(dir: &Path) -> AskAnswer {
    let answered = dir.join("answered");
    waiting::task::until(Duration::from_secs(10), "no answer came", || answered.exists()).await;
    frame::decode(&std::fs::read(&answered).unwrap()).unwrap()
}
