//! Starting a worker, against the local provider with a script in place of the worker.

use std::os::unix::process::CommandExt;
use std::path::Path;

use process::ProcessRecord;
use protocol::frame;
use protocol::paths::WORKER_RECORD_FILE;
use protocol::worker::{AskId, AskOutcome, AskReply, WorkerAsk, WorkerOutcome};
use providers::local::{LocalConfig, LocalProvider};

use super::*;
use crate::provider_set::ProviderSet;
use crate::store::tests::store_with_session;

/// A local provider whose worker is `script`.
fn provider_running(dir: &Path, script: &str) -> Arc<dyn Provider> {
    use std::os::unix::fs::PermissionsExt;
    let exe = dir.join("worker");
    std::fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: exe,
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        shell: Some("/bin/sh".into()),
        path: None,
    }))
}

/// A hub whose store has the workspace `w`, ready at `/w`, and its session `s`, and whose workers `provider` starts.
fn hub_using(dir: &Path, provider: &Arc<dyn Provider>) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main", None).unwrap();
    let providers = ProviderSet::only(provider.clone());
    Arc::new(Hub::new(store, dir.join("attachments"), dir.join("logs"), providers, Vec::new(), false))
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
    let instance_id: InstanceId = "i".into();
    provider.create(&instance_id, None, None, "i").await.unwrap();
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
    let instance_id: InstanceId = "i".into();
    provider.create(&instance_id, None, None, "i").await.unwrap();
    let hub = hub_using(dir.path(), &provider);
    let asked = std::time::Instant::now();

    let opened =
        WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log"), Opening::Started)
            .await;

    let Err(e) = opened else { panic!("the hello's reply was read") };
    assert_eq!(e.code, ErrorCode::Worker, "{e:?}");
    assert!(asked.elapsed() < HELLO_TIMEOUT / 2, "it waited for the time limit");
}

/// One that chose a version this hub doesn't speak, which no worker should, since a worker chooses one both speak.
#[tokio::test]
async fn a_worker_that_chose_another_version_is_not_used() {
    let dir = tempfile::tempdir().unwrap();
    let reply = r#"{"frame":"reply","id":1,"response":{"result":"hello","protocol_version":9999}}"#;
    let provider = provider_running(dir.path(), &format!("read call\necho '{reply}'\ncat >/dev/null"));
    let instance_id: InstanceId = "i".into();
    provider.create(&instance_id, None, None, "i").await.unwrap();
    let hub = hub_using(dir.path(), &provider);

    let opened =
        WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log"), Opening::Started)
            .await;

    let Err(e) = opened else { panic!("it was used") };
    assert_eq!(e.code, ErrorCode::VersionMismatch, "{e:?}");
}

#[tokio::test]
async fn a_worker_asking_for_its_pull_request_is_answered_once_it_has_answered_the_calls_that_takes() {
    let dir = tempfile::tempdir().unwrap();
    let hello = WorkerResponse::Hello { protocol_version: versions::WORKERS.newest, build: None };
    let no_limit = WorkerOutcome::Error(ApiError::new(ErrorCode::Unavailable, "GitHub can't be reached"));
    let none = WorkerResponse::PullRequest { pull_request: None, branch: None };
    frames_in(
        dir.path(),
        &[
            ("hello", WorkerFrame::Reply { id: 1, outcome: WorkerOutcome::Response(hello) }),
            ("ask", WorkerFrame::Ask { id: AskId::new(9), session_id: "s".into(), ask: WorkerAsk::PullRequest }),
            // How much of GitHub's limit is left, then the pull request.
            ("limit", WorkerFrame::Reply { id: 2, outcome: no_limit }),
            ("found", WorkerFrame::Reply { id: 3, outcome: WorkerOutcome::Response(none) }),
        ],
    );
    let d = dir.path().display();
    let script = format!(
        "read -r call\ncat '{d}/hello'\ncat '{d}/ask'\nread -r call\ncat '{d}/limit'\nread -r call\ncat '{d}/found'\n{}",
        answer_to_file(dir.path())
    );
    let provider = provider_running(dir.path(), &script);
    provider.create(&"i".into(), None, None, "i").await.unwrap();
    let hub = hub_using(dir.path(), &provider);

    let link =
        WorkerLink::open(hub, provider, "w".into(), "i".into(), &dir.path().join("logs/i.log"), Opening::Started)
            .await
            .unwrap();

    let answer = answered(dir.path()).await;
    let outcome = AskOutcome::Response(AskReply::PullRequest { pull_request: None });
    assert_eq!(answer, AskAnswer { answer: AskId::new(9), outcome });
    link.kill();
}

/// As from a newer worker, after a rollback, so the agent's call is refused at once rather than left for the worker
/// to give up on.
#[tokio::test]
async fn an_ask_this_hub_cant_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hello = WorkerResponse::Hello { protocol_version: versions::WORKERS.newest, build: None };
    frames_in(dir.path(), &[("hello", WorkerFrame::Reply { id: 1, outcome: WorkerOutcome::Response(hello) })]);
    let ask = r#"{"frame":"ask","id":9,"session_id":{"newer":true},"ask":{"method":"pull_request"}}"#;
    let d = dir.path().display();
    let script = format!("read -r call\ncat '{d}/hello'\necho '{ask}'\n{}", answer_to_file(dir.path()));
    let provider = provider_running(dir.path(), &script);
    provider.create(&"i".into(), None, None, "i").await.unwrap();
    let hub = hub_using(dir.path(), &provider);

    let link =
        WorkerLink::open(hub, provider, "w".into(), "i".into(), &dir.path().join("logs/i.log"), Opening::Started)
            .await
            .unwrap();

    let answer = answered(dir.path()).await;
    assert_eq!(answer.answer, AskId::new(9));
    assert!(matches!(&answer.outcome, AskOutcome::Error(e) if e.code == ErrorCode::Unsupported), "{answer:?}");
    link.kill();
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
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !answered.exists() {
        assert!(tokio::time::Instant::now() < deadline, "no answer came");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    frame::decode(&std::fs::read(&answered).unwrap()).unwrap()
}
