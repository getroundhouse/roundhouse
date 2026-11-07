//! The adapter's process handling, with small scripts standing in for the CLI.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use agents::claude::spawn_at;
use agents::{AgentOptions, AgentOutput};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::AgentKind;
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Through `install`, because writing an executable from this multithreaded process can break running
/// it on Linux. A fork in another test briefly inherits the open file, and `exec` then fails with
/// "text file busy". This way, this process never has the file open for writing.
fn script(dir: &Path, body: &str) -> PathBuf {
    let source = dir.join("fake-cli.source");
    std::fs::write(&source, format!("#!/bin/sh\n{body}\n")).unwrap();
    let path = dir.join("fake-cli");
    let status = std::process::Command::new("install").args(["-m", "755"]).arg(&source).arg(&path).status().unwrap();
    assert!(status.success());
    path
}

fn spec(dir: &Path) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: 1,
        agent: AgentKind::Claude,
        cwd: dir.to_string_lossy().into_owned(),
        agent_session_id: "s".into(),
        resume: false,
        model: None,
        permission_mode: None,
        env: vec![],
    }
}

async fn until_exit(output: &mut mpsc::Receiver<AgentOutput>) -> Vec<AgentOutput> {
    let mut seen = Vec::new();
    loop {
        let item = timeout(Duration::from_secs(20), output.recv()).await.expect("the agent to exit").expect("output");
        let done = matches!(item, AgentOutput::Exited { .. });
        seen.push(item);
        if done {
            return seen;
        }
    }
}

#[tokio::test]
async fn stopping_an_agent_that_has_already_exited_returns_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let agent = spawn_at(script(dir.path(), "exit 3"), spec(dir.path()), tx, AgentOptions::default()).unwrap();
    until_exit(&mut rx).await;
    timeout(Duration::from_secs(5), agent.stop()).await.expect("stop must not wait on a process that's gone");
}

#[tokio::test]
async fn stopping_an_agent_ends_it_and_reports_that_it_was_asked_to() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let agent = spawn_at(script(dir.path(), "exec sleep 100"), spec(dir.path()), tx, AgentOptions::default()).unwrap();
    timeout(Duration::from_secs(5), agent.stop()).await.expect("stop to finish");
    let last = until_exit(&mut rx).await.pop().unwrap();
    assert!(matches!(last, AgentOutput::Exited { requested: true, signal: Some(15), .. }), "{last:?}");
}

#[tokio::test]
async fn output_that_is_not_valid_utf8_does_not_stop_the_stream() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let body = r#"
printf '{"type":"system","subtype":"init","session_id":"s"}\n'
printf '\377\376 not text\n'
printf '{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"after"}]}}\n'
"#;
    let _agent = spawn_at(script(dir.path(), body), spec(dir.path()), tx, AgentOptions::default()).unwrap();
    let seen = until_exit(&mut rx).await;
    let got_it = seen.iter().any(|item| match item {
        AgentOutput::Event { body: EventBody::Assistant { blocks, .. }, .. } => {
            matches!(blocks.first(), Some(ContentBlock::Text { text }) if text == "after")
        }
        _ => false,
    });
    assert!(got_it, "{seen:?}");
}

#[tokio::test]
async fn the_pid_file_records_the_process_and_goes_away_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let pid_file = dir.path().join("agents/s.pid");
    let options = AgentOptions { pid_file: Some(pid_file.clone()), ..AgentOptions::default() };
    let agent = spawn_at(script(dir.path(), "exec sleep 100"), spec(dir.path()), tx, options).unwrap();

    let recorded = std::fs::read_to_string(&pid_file).unwrap();
    let mut lines = recorded.lines();
    assert_eq!(lines.next().unwrap().parse::<u32>().unwrap(), agent.pid());
    assert!(!lines.next().unwrap().is_empty(), "the start time guards against a reused PID");

    agent.stop().await;
    until_exit(&mut rx).await;
    assert!(!pid_file.exists());
}
