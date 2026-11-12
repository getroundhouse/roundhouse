//! Starting a worker, against the local provider with a script in place of the worker.

use std::os::unix::process::CommandExt;
use std::path::Path;

use process::ProcessRecord;
use protocol::paths::WORKER_RECORD_FILE;
use protocol::worker::WorkerOutcome;
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

/// A hub with one session, whose only provider is `provider`.
fn hub_using(dir: &Path, provider: &Arc<dyn Provider>) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    Arc::new(Hub::new(store, dir.join("attachments"), dir.join("logs"), ProviderSet::only(provider.clone()), false))
}

#[tokio::test]
async fn a_worker_refused_its_instance_leaves_the_one_serving_it_running() {
    let dir = tempfile::tempdir().unwrap();
    let refusal = WorkerFrame::Reply {
        id: 1,
        outcome: WorkerOutcome::Error(ApiError::new(ErrorCode::Conflict, "another worker is serving it")),
    };
    std::fs::write(dir.path().join("refusal"), frame::encode(&refusal).unwrap()).unwrap();
    let provider = provider_running(
        dir.path(),
        &format!("read call\ncat '{}'\ncat >/dev/null", dir.path().join("refusal").display()),
    );
    let instance_id: InstanceId = "i".into();
    provider.create(&instance_id, None, None, "i").await.unwrap();
    let mut serving = std::process::Command::new("sleep").arg("100").process_group(0).spawn().unwrap();
    ProcessRecord::write(&dir.path().join("instances/i").join(WORKER_RECORD_FILE), serving.id()).unwrap();
    let hub = hub_using(dir.path(), &provider);

    let opened = WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log")).await;

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

    let opened = WorkerLink::open(hub, provider, "w".into(), instance_id, &dir.path().join("logs/i.log")).await;

    let Err(e) = opened else { panic!("the hello's reply was read") };
    assert_eq!(e.code, ErrorCode::Worker, "{e:?}");
    assert!(asked.elapsed() < HELLO_TIMEOUT / 2, "it waited for the time limit");
}
