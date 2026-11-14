//! The adapter's process handling, with small scripts standing in for the CLI.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use agents::claude::spawn_at;
use agents::{AgentOptions, AgentOutput, ToolServer};
use protocol::event::{CompactedBy, CompactionOutcome, ContentBlock, EventBody, NoticeLevel};
use protocol::model::{AgentKind, PermissionMode, RunId, Seq};
use protocol::prompt::Prompt;
use protocol::worker::{AgentSpec, ToolsToken};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// A script standing in for the CLI, which runs `body`.
fn script(dir: &Path, body: &str) -> PathBuf {
    agents::conformance::executable(dir, "fake-cli", &format!("#!/bin/sh\n{body}\n")).unwrap()
}

fn spec(dir: &Path) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: RunId::new(1),
        agent: AgentKind::Claude,
        cwd: dir.to_string_lossy().into_owned(),
        start: protocol::capability::conversation::Start::New { conversation: Some("s".into()) },
        settings: agents::claude::description().defaults,
        env: vec![],
        tools_token: None,
        login: None,
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
    let agent = spawn_at(script(dir.path(), "exit 3"), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();
    until_exit(&mut rx).await;
    timeout(Duration::from_secs(5), agent.stop()).await.expect("stop must not wait on a process that's gone");
}

#[tokio::test]
async fn stopping_an_agent_ends_it_and_reports_that_it_was_asked_to() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let agent =
        spawn_at(script(dir.path(), "exec sleep 100"), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();
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
    let _agent = spawn_at(script(dir.path(), body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();
    let seen = until_exit(&mut rx).await;
    let got_it = seen.iter().any(|item| match item {
        AgentOutput::Event { body: EventBody::Assistant { blocks, .. }, .. } => {
            matches!(blocks.first(), Some(ContentBlock::Text { text, .. }) if text == "after")
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

/// Copies the file it's given as `--mcp-config` to `given`, if it's given one, then waits to be stopped.
const KEEPS_ITS_MCP_CONFIG: &str = r#"while [ $# -gt 0 ] && [ "$1" != --mcp-config ]; do shift; done
[ $# -gt 0 ] && cp "$2" given.tmp && mv given.tmp given; exec sleep 100"#;

/// What the CLI wrote to `given`, such as the file it was given as `--mcp-config`, once it has written it.
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
    let cli = script(dir.path(), KEEPS_ITS_MCP_CONFIG);
    let agent = spawn_at(cli, spec(dir.path()), tx, with_tools(&config)).await.unwrap();

    assert_eq!(given(dir.path()).await, std::fs::read(&config).unwrap());
    agent.stop().await;
    until_exit(&mut rx).await;
    assert!(!config.exists(), "the token isn't left on disk");
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
{KEEPS_ITS_MCP_CONFIG}"#
    );
    let mut resumed = spec(dir.path());
    resumed.start = protocol::capability::conversation::Start::Resume { conversation: "s".into() };

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
    let config = dir.path().join("agents/s-1.mcp.json");

    let started = spawn_at(dir.path().join("no-such-cli"), spec(dir.path()), tx, with_tools(&config)).await;

    assert!(started.is_err());
    assert!(!config.exists(), "the token isn't left on disk");
}

fn in_plan_mode(dir: &Path) -> AgentSpec {
    let mut spec = spec(dir);
    spec.settings.permission_mode = PermissionMode::Plan;
    spec
}

/// Reads the first two lines the agent is sent into `lines`, then waits to be stopped.
const READS_TWO_LINES: &str = "read first; read second; printf '%s\\n%s\\n' \"$first\" \"$second\" > lines.tmp; mv lines.tmp lines; exec sleep 100";

/// The two lines [`READS_TWO_LINES`] read.
async fn two_lines_read(dir: &Path) -> Vec<serde_json::Value> {
    let read = dir.join("lines");
    timeout(Duration::from_secs(10), async {
        while !read.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the script to read two lines");
    let text = std::fs::read_to_string(&read).unwrap();
    text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn asked_into_plan_mode_then_prompted(lines: &[serde_json::Value]) {
    assert_eq!(lines[0]["request"], serde_json::json!({"subtype": "set_permission_mode", "mode": "plan"}));
    assert_eq!(lines[1]["type"], "user", "the prompt comes second");
}

#[tokio::test]
async fn an_agent_starting_in_plan_mode_is_asked_into_it_before_its_first_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);

    let agent = spawn_at(script(dir.path(), READS_TWO_LINES), in_plan_mode(dir.path()), tx, AgentOptions::default())
        .await
        .unwrap();
    agent.send_prompt(Seq::new(1), &Prompt::new("go")).await.unwrap();

    asked_into_plan_mode_then_prompted(&two_lines_read(dir.path()).await);
    agent.stop().await;
}

/// A launch that has to be corrected, here because the conversation to resume isn't there, is a new process, which
/// starts with full access too.
#[tokio::test]
async fn an_agent_relaunched_in_plan_mode_is_asked_into_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);
    let body = format!(
        r#"case " $* " in *" --resume "*) read line; echo "No conversation found with session ID: s" >&2; exit 1;; esac
{READS_TWO_LINES}"#
    );
    let mut resumed = in_plan_mode(dir.path());
    resumed.start = protocol::capability::conversation::Start::Resume { conversation: "s".into() };

    let agent = spawn_at(script(dir.path(), &body), resumed, tx, AgentOptions::default()).await.unwrap();
    agent.send_prompt(Seq::new(1), &Prompt::new("go")).await.unwrap();

    asked_into_plan_mode_then_prompted(&two_lines_read(dir.path()).await);
    agent.stop().await;
}

/// Its switch into plan mode is how it starts there, not a change to report. A later one is.
#[tokio::test]
async fn an_agent_starting_in_plan_mode_reports_only_the_changes_after() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let status = |mode: &str| format!(r#"printf '{{"type":"system","subtype":"status","permissionMode":"{mode}"}}\n'"#);
    let body = format!(
        r#"read line
id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"%s","response":{{}}}}}}\n' "$id"
{}
printf '{{"type":"system","subtype":"init","session_id":"s","permissionMode":"plan"}}\n'
{}
{}"#,
        status("plan"),
        status("bypassPermissions"),
        status("plan"),
    );

    let _agent =
        spawn_at(script(dir.path(), &body), in_plan_mode(dir.path()), tx, AgentOptions::default()).await.unwrap();

    let reported: Vec<PermissionMode> = until_exit(&mut rx)
        .await
        .into_iter()
        .filter_map(|item| match item {
            AgentOutput::Event { body: EventBody::AgentMode { permission_mode }, .. } => Some(permission_mode),
            _ => None,
        })
        .collect();
    assert_eq!(reported, [PermissionMode::FullAccess, PermissionMode::Plan]);
}

#[tokio::test]
async fn an_agent_that_refuses_plan_mode_is_stopped_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let body = r#"read line
id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
printf '{"type":"control_response","response":{"subtype":"error","request_id":"%s","error":"no plans here"}}\n' "$id"
exec sleep 100"#;

    let _agent =
        spawn_at(script(dir.path(), body), in_plan_mode(dir.path()), tx, AgentOptions::default()).await.unwrap();

    let seen = until_exit(&mut rx).await;
    let said = seen.iter().any(|item| {
        matches!(item, AgentOutput::Event { body: EventBody::Notice { level: NoticeLevel::Error, message, .. }, .. }
            if message.contains("no plans here"))
    });
    assert!(said, "{seen:?}");
    assert!(matches!(seen.last(), Some(AgentOutput::Exited { signal: Some(15), requested: false, .. })), "{seen:?}");
}

/// One that starts a turn first may be about to use a tool with full access. It's stopped before the turn's start goes
/// on, since that would say the chat isn't planning.
#[tokio::test]
async fn an_agent_that_starts_on_a_prompt_before_going_into_plan_mode_is_stopped_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let body = r#"read line
printf '{"type":"system","subtype":"init","session_id":"s","permissionMode":"bypassPermissions"}\n'
exec sleep 100"#;

    let _agent =
        spawn_at(script(dir.path(), body), in_plan_mode(dir.path()), tx, AgentOptions::default()).await.unwrap();

    let seen = until_exit(&mut rx).await;
    let said = seen.iter().any(|item| {
        matches!(item, AgentOutput::Event { body: EventBody::Notice { level: NoticeLevel::Error, message, .. }, .. }
            if message.contains("plan mode"))
    });
    assert!(said, "{seen:?}");
    let started = seen.iter().any(|item| matches!(item, AgentOutput::Event { body: EventBody::AgentInit { .. }, .. }));
    assert!(!started, "{seen:?}");
    assert!(matches!(seen.last(), Some(AgentOutput::Exited { signal: Some(15), requested: false, .. })), "{seen:?}");
}

/// Only the adapter knows how Claude is asked to compact, and which recorded request each line it sent is, so the
/// compaction names the request, and what it said to keep, and the turn's end says it was only that.
#[tokio::test]
async fn a_compaction_names_the_request_for_it_and_its_turn_says_it_was_only_that() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(64);
    // Takes the first prompt it's sent, by the uuid it came with, and compacts as the CLI does, if it's `/compact`.
    let body = r#"
while read -r line; do case "$line" in *'"type":"user"'*) break;; esac; done
case "$line" in *'"/compact Keep the API design"'*) ;; *) exit 1;; esac
uuid=$(printf '%s' "$line" | sed 's/.*"uuid":"\([^"]*\)".*/\1/')
printf '{"type":"command_lifecycle","command_uuid":"%s","state":"started"}\n' "$uuid"
printf '{"type":"system","subtype":"status","status":"compacting"}\n'
printf '{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual","pre_tokens":75284}}\n'
printf '{"type":"user","isSynthetic":true,"message":{"role":"user","content":"Continued.\\nSummary:\\n1. What was asked"}}\n'
printf '{"type":"result","subtype":"success","num_turns":0,"result":""}\n'
"#;
    let agent = spawn_at(script(dir.path(), body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();

    agent.compact(Seq::new(7), Some("Keep the API design")).await.unwrap();

    let seen = until_exit(&mut rx).await;
    let compacted = seen.iter().find_map(|item| match item {
        AgentOutput::Event { body: EventBody::AgentCompacted { asked, keeping, outcome }, .. } => {
            Some((*asked, keeping.clone(), outcome.clone()))
        }
        _ => None,
    });
    let only = seen
        .iter()
        .any(|item| matches!(item, AgentOutput::Event { body: EventBody::TurnEnded { only_compacted: true, .. }, .. }));
    let (asked, keeping, outcome) = compacted.expect("a compaction");
    assert_eq!((asked, keeping.as_deref()), (Some(Seq::new(7)), Some("Keep the API design")));
    let CompactionOutcome::Compacted { by, summary, .. } = outcome else { panic!("{outcome:?}") };
    assert_eq!((by, summary.as_deref()), (CompactedBy::Asked, Some("1. What was asked")));
    assert!(only, "{seen:?}");
}
