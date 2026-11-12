//! The worker's main loop, driven directly over an in-memory stream.

#![cfg(test)]

use std::time::Duration;

use protocol::PROTOCOL_VERSION;
use protocol::client::ErrorCode;
use protocol::frame;
use protocol::model::{AgentKind, RunId, SessionSettings};
use protocol::worker::{AgentSpec, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;
use worker::{Config, Deleter, serve};

#[tokio::test]
async fn a_start_racing_the_hub_going_away_cannot_leave_the_worker_running() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let cli = dir.path().join("fake-cli");
    std::fs::write(&cli, "#!/bin/sh\nexec sleep 100\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();

    for attempt in 0..8 {
        let (hub_end, worker_end) = tokio::io::duplex(64 * 1024);
        let (worker_read, worker_write) = tokio::io::split(worker_end);
        let (mut hub_read, mut hub_write) = tokio::io::split(hub_end);
        let config = Config {
            workspaces_root: dir.path().to_path_buf(),
            state_dir: None,
            home: None,
            path: None,
            claude_bin: Some(cli.clone()),
            scratch_dir: dir.path().to_path_buf(),
            deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
            refused: None,
        };
        let running = tokio::spawn(serve(worker_read, worker_write, config));
        tokio::spawn(async move {
            let mut sink = Vec::new();
            let _ = hub_read.read_to_end(&mut sink).await;
        });

        let calls = [
            WorkerRequest::Hello { protocol_version: PROTOCOL_VERSION },
            WorkerRequest::AgentStart(AgentSpec {
                session_id: format!("s{attempt}").into(),
                run: RunId::new(1),
                agent: AgentKind::Claude,
                cwd: dir.path().to_string_lossy().into_owned(),
                agent_session_id: "a".into(),
                resume: false,
                settings: SessionSettings::default(),
                env: vec![],
            }),
        ];
        for (id, request) in calls.into_iter().enumerate() {
            hub_write.write_all(&frame::encode(&WorkerCall { id: id as u64, request }).unwrap()).await.unwrap();
        }
        // Goes away with the start still in flight.
        hub_write.shutdown().await.unwrap();
        drop(hub_write);

        timeout(Duration::from_secs(10), running)
            .await
            .unwrap_or_else(|_| panic!("attempt {attempt}: the worker didn't exit"))
            .unwrap();
    }
}

/// Each request runs on a task of its own, so the stop could be handled before the setup it follows. On
/// the same runtime as the worker, which tends to run the task it spawned last first.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_right_behind_a_setup_stops_it_even_before_its_script_starts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".conductor")).unwrap();
    std::fs::write(dir.path().join(".conductor/settings.toml"), "[scripts]\nsetup = \"sleep 60\"\n").unwrap();
    let (hub_end, worker_end) = tokio::io::duplex(64 * 1024);
    let (worker_read, worker_write) = tokio::io::split(worker_end);
    let (hub_read, mut hub_write) = tokio::io::split(hub_end);
    let config = Config {
        workspaces_root: dir.path().to_path_buf(),
        state_dir: None,
        home: None,
        path: None,
        claude_bin: None,
        scratch_dir: dir.path().to_path_buf(),
        deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
        refused: None,
    };
    tokio::spawn(serve(worker_read, worker_write, config));
    let path = dir.path().to_string_lossy().into_owned();
    let calls = [
        WorkerRequest::Hello { protocol_version: PROTOCOL_VERSION },
        WorkerRequest::SetupWorkspace { repo_path: path.clone(), path, env: vec![] },
        WorkerRequest::CancelSetup,
    ];
    let mut lines = Vec::new();
    for (id, request) in calls.into_iter().enumerate() {
        lines.extend(frame::encode(&WorkerCall { id: id as u64, request }).unwrap());
    }

    // In one write, so the worker reads the stop before the setup has done anything.
    hub_write.write_all(&lines).await.unwrap();

    let mut replies = BufReader::new(hub_read).lines();
    let setup = timeout(Duration::from_secs(20), async {
        loop {
            let line = replies.next_line().await.unwrap().expect("a reply to the setup");
            if let WorkerFrame::Reply { id: 1, outcome } = frame::decode(line.as_bytes()).unwrap() {
                return outcome;
            }
        }
    })
    .await
    .expect("the script should have been stopped");
    assert_eq!(setup, WorkerOutcome::Response(WorkerResponse::SetupDone { code: None }));
}

/// From a newer hub: one it doesn't know, and a known one with a value it can't read. Either is refused, so the hub
/// does without it, rather than left for the hub's call to wait on until the connection closes.
#[tokio::test]
async fn a_request_from_a_newer_hub_is_refused_and_the_worker_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let (hub_end, worker_end) = tokio::io::duplex(64 * 1024);
    let (worker_read, worker_write) = tokio::io::split(worker_end);
    let (hub_read, mut hub_write) = tokio::io::split(hub_end);
    let config = Config {
        workspaces_root: dir.path().to_path_buf(),
        state_dir: None,
        home: None,
        path: None,
        claude_bin: None,
        scratch_dir: dir.path().to_path_buf(),
        deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
        refused: None,
    };
    tokio::spawn(serve(worker_read, worker_write, config));
    let hello =
        frame::encode(&WorkerCall { id: 0, request: WorkerRequest::Hello { protocol_version: PROTOCOL_VERSION } });
    hub_write.write_all(&hello.unwrap()).await.unwrap();
    hub_write.write_all(b"{\"id\":1,\"request\":{\"method\":\"from_a_newer_hub\",\"path\":\"a\"}}\n").await.unwrap();
    hub_write
        .write_all(b"{\"id\":2,\"request\":{\"method\":\"account_for\",\"agent\":{\"newer\":true}}}\n")
        .await
        .unwrap();
    let again =
        frame::encode(&WorkerCall { id: 3, request: WorkerRequest::Hello { protocol_version: PROTOCOL_VERSION } });
    hub_write.write_all(&again.unwrap()).await.unwrap();

    let mut replies = BufReader::new(hub_read).lines();
    let mut answered = std::collections::BTreeMap::new();
    while answered.len() < 4 {
        let line = timeout(Duration::from_secs(10), replies.next_line()).await.expect("a reply").unwrap().unwrap();
        if let WorkerFrame::Reply { id, outcome } = frame::decode(line.as_bytes()).unwrap() {
            answered.insert(id, outcome);
        }
    }

    for (id, says) in [(1, "doesn't know that request"), (2, "can't read that request")] {
        let refused = &answered[&id];
        assert!(
            matches!(refused, WorkerOutcome::Error(e) if e.code == ErrorCode::Unsupported && e.message.contains(says)),
            "{id}: {refused:?}"
        );
    }
    assert!(matches!(answered[&3], WorkerOutcome::Response(WorkerResponse::Hello { .. })), "it carried on");
}
