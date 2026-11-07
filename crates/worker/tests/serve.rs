//! The worker's main loop, driven directly over an in-memory stream.

#![cfg(test)]

use std::time::Duration;

use protocol::PROTOCOL_VERSION;
use protocol::frame;
use protocol::model::AgentKind;
use protocol::worker::{AgentSpec, WorkerCall, WorkerRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use worker::{Config, serve};

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
                run: 1,
                agent: AgentKind::Claude,
                cwd: dir.path().to_string_lossy().into_owned(),
                agent_session_id: "a".into(),
                resume: false,
                model: None,
                permission_mode: None,
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
