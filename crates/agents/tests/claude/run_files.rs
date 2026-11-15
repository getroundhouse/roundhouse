//! The files a run of the CLI keeps on disk while it runs, with small scripts standing in for the CLI.

#![cfg(test)]

use std::path::Path;
use std::time::Duration;

use agents::claude::spawn_at;
use agents::{AgentError, AgentOptions, Instructions, ToolServer};
use protocol::capability::conversation::Start;
use protocol::worker::ToolsToken;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::process::{script, spec, until_exit};

#[tokio::test]
async fn the_pid_file_records_the_process_and_goes_away_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let pid_file = dir.path().join("agents/s.pid");
    let options = AgentOptions { pid_file: Some(pid_file.clone()), ..AgentOptions::default() };
    let agent = spawn_at(script(dir.path(), "exec sleep 100"), spec(dir.path()), tx, options).await.unwrap();

    let recorded = std::fs::read_to_string(&pid_file).unwrap();
    let mut lines = recorded.lines();
    assert_eq!(lines.next().unwrap().parse::<u32>().unwrap(), agent.pid());
    assert!(!lines.next().unwrap().is_empty(), "the start time guards against a reused PID");

    agent.stop().await;
    until_exit(&mut rx).await;
    assert!(!pid_file.exists());
}

/// Options that tell the agent of the worker's tools in `config`.
fn with_tools(config: &Path) -> AgentOptions {
    let tools = ToolServer {
        url: "http://127.0.0.1:50123/mcp".into(),
        token: ToolsToken::new("token-s"),
        config: config.to_path_buf(),
    };
    AgentOptions { tools: Some(tools), ..AgentOptions::default() }
}

/// Options that tell the agent Roundhouse's instructions in `file`.
fn with_instructions(file: &Path) -> AgentOptions {
    let told = Instructions { text: "Keep scratch in `.context/`.".into(), file: file.to_path_buf() };
    AgentOptions { instructions: Some(told), ..AgentOptions::default() }
}

/// Copies the file it's given with `flag` to `given`, if it's given one, then waits to be stopped.
fn keeps_the_file_given_with(flag: &str) -> String {
    format!(
        r#"while [ $# -gt 0 ] && [ "$1" != {flag} ]; do shift; done
[ $# -gt 0 ] && cp "$2" given.tmp && mv given.tmp given; exec sleep 100"#
    )
}

/// What the CLI wrote to `given`, such as a file it was given, once it has written it.
async fn given(dir: &Path) -> Vec<u8> {
    let given = dir.join("given");
    timeout(Duration::from_secs(10), async {
        while !given.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the CLI to be given the file");
    std::fs::read(given).unwrap()
}

#[tokio::test]
async fn the_cli_is_told_of_the_workers_tools_in_a_file_that_goes_away_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let config = dir.path().join("agents/s-1.mcp.json");
    let cli = script(dir.path(), &keeps_the_file_given_with("--mcp-config"));
    let agent = spawn_at(cli, spec(dir.path()), tx, with_tools(&config)).await.unwrap();

    assert_eq!(given(dir.path()).await, std::fs::read(&config).unwrap());
    agent.stop().await;
    until_exit(&mut rx).await;
    assert!(!config.exists(), "the token isn't left on disk");
}

/// Where the CLI reads them for its subagents, which is where it reads them for the agent too. A launch that resumes a
/// conversation reads them as one that starts it does, since a compacted conversation takes what that launch gives.
#[tokio::test]
async fn the_cli_reads_its_instructions_from_a_file_that_goes_away_with_it() {
    for resume in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let file = dir.path().join("agents/s-1.instructions.txt");
        let cli = script(dir.path(), &keeps_the_file_given_with("--append-subagent-system-prompt-file"));
        let mut launched = spec(dir.path());
        if resume {
            launched.start = Start::Resume { conversation: "s".into() };
        }

        let agent = spawn_at(cli, launched, tx, with_instructions(&file)).await.unwrap();

        assert_eq!(given(dir.path()).await, b"Keep scratch in `.context/`.", "resuming: {resume}");
        agent.stop().await;
        until_exit(&mut rx).await;
        assert!(!file.exists(), "resuming: {resume}");
    }
}

/// Rather than start an agent that goes on without them, since a chat keeps what it was first told.
#[tokio::test]
async fn a_launch_whose_instructions_cant_be_written_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);
    std::fs::write(dir.path().join("agents"), "a file where their folder would be").unwrap();
    let options = with_instructions(&dir.path().join("agents/s-1.instructions.txt"));

    let started = spawn_at(script(dir.path(), "exec sleep 100"), spec(dir.path()), tx, options).await;

    assert!(matches!(started, Err(AgentError::Instructions { .. })), "{:?}", started.err());
}

/// The CLI is started again with the other flag when it can't find the conversation, and that launch needs the file
/// too, since a CLI given one that isn't there doesn't start.
#[tokio::test]
async fn a_launch_started_again_is_told_of_the_tools_too() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);
    let config = dir.path().join("agents/s-1.mcp.json");
    let body = format!(
        r#"case " $* " in *" --resume "*) echo "No conversation found with session ID: s" >&2; exit 1;; esac
{}"#,
        keeps_the_file_given_with("--mcp-config")
    );
    let mut resumed = spec(dir.path());
    resumed.start = Start::Resume { conversation: "s".into() };

    let agent = spawn_at(script(dir.path(), &body), resumed, tx, with_tools(&config)).await.unwrap();

    assert_eq!(given(dir.path()).await, std::fs::read(&config).unwrap());
    agent.stop().await;
}

/// So a call to one is offered MCP's tasks, which a wait is answered with. One that isn't is left as it was.
#[tokio::test]
async fn a_cli_told_of_the_tools_takes_tasks_from_them() {
    for (with, told) in [(true, "1"), (false, "unset")] {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let options = if with { with_tools(&dir.path().join("agents/s-1.mcp.json")) } else { AgentOptions::default() };
        let says =
            r#"printf '%s' "${CLAUDE_AUTO_BACKGROUND_TASKS-unset}" > given.tmp; mv given.tmp given; exec sleep 100"#;

        let agent = spawn_at(script(dir.path(), says), spec(dir.path()), tx, options).await.unwrap();

        assert_eq!(given(dir.path()).await, told.as_bytes(), "with the tools: {with}");
        agent.stop().await;
    }
}

#[tokio::test]
async fn a_launch_that_cant_start_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);
    let (config, file) = (dir.path().join("agents/s-1.mcp.json"), dir.path().join("agents/s-1.instructions.txt"));
    let options = AgentOptions { tools: with_tools(&config).tools, ..with_instructions(&file) };

    let started = spawn_at(dir.path().join("no-such-cli"), spec(dir.path()), tx, options).await;

    assert!(started.is_err());
    assert!(!config.exists(), "the token isn't left on disk");
    assert!(!file.exists());
}
