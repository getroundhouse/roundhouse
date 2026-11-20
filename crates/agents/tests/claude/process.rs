//! The adapter's process handling, with small scripts standing in for the CLI.

#![cfg(test)]

use agents::testing::Asking;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agents::claude::spawn_at;
use agents::{AgentError, AgentOptions, AgentOutput};
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::{Effort, Settings};
use protocol::event::{AgentRunState, CompactedBy, CompactionOutcome, ContentBlock, EventBody, NoticeLevel};
use protocol::fixtures;
use protocol::model::{AgentKind, RunId, Seq};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// A script standing in for the CLI, which runs `body`.
pub(crate) fn script(dir: &Path, body: &str) -> PathBuf {
    agents::conformance::executable(dir, "fake-cli", &format!("#!/bin/sh\n{body}\n")).unwrap()
}

pub(crate) fn spec(dir: &Path) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: RunId::new(1),
        agent: AgentKind::Claude,
        cwd: dir.to_string_lossy().into_owned(),
        start: protocol::capability::conversation::Start::New { conversation: Some("s".into()) },
        settings: agents::claude::description().defaults(),
        env: vec![],
        tools_token: None,
        login: None,
        repository: None,
    }
}

pub(crate) async fn until_exit(output: &mut mpsc::Receiver<AgentOutput>) -> Vec<AgentOutput> {
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

/// The CLI reports `init` only as a turn starts, so stopping the agent before its first turn gets none, and the prompt
/// it dropped has to be said without one, while the process is still up, or its chat stays at work on a prompt no
/// agent has.
#[tokio::test]
async fn a_prompt_cancelled_before_the_first_turn_is_said_to_be_dropped_without_the_agent_exiting() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    // What the CLI wrote when it was sent a prompt, then an interrupt, as it started, then stays up as it did.
    let body = r#"
while read -r line; do case "$line" in *'"type":"user"'*) break;; esac; done
uuid=$(printf '%s' "$line" | sed 's/.*"uuid":"\([^"]*\)".*/\1/')
printf '{"type":"system","subtype":"session_state_changed","state":"running"}\n'
printf '{"type":"command_lifecycle","command_uuid":"%s","state":"queued"}\n' "$uuid"
printf '{"type":"command_lifecycle","command_uuid":"%s","state":"cancelled"}\n' "$uuid"
printf '{"type":"system","subtype":"session_state_changed","state":"idle"}\n'
exec sleep 100"#;
    let agent = spawn_at(script(dir.path(), body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();

    agent.prompt(Seq::new(1), &Prompt::new("go")).await.unwrap();

    let mut said = Vec::new();
    while said.len() < 3 {
        let item = timeout(Duration::from_secs(10), rx.recv()).await.expect("what the CLI said").expect("output");
        let AgentOutput::Event { body, .. } = item else { panic!("{item:?}") };
        said.push(body);
    }
    let state = |state| EventBody::AgentState { state };
    let dropped = EventBody::PromptDropped { prompt: Seq::new(1) };
    assert_eq!(said, [state(AgentRunState::Running), dropped, state(AgentRunState::Idle)]);
    agent.stop().await;
}

fn in_plan_mode(dir: &Path) -> AgentSpec {
    let defaults = agents::claude::description().defaults();
    AgentSpec { settings: Settings { mode: Some(fixtures::plan_mode()), ..defaults }, ..spec(dir) }
}

/// Reads the first two lines the agent is sent into `lines`, then waits to be stopped.
const READS_TWO_LINES: &str = "read first; read second; printf '%s\\n%s\\n' \"$first\" \"$second\" > lines.tmp; mv lines.tmp lines; exec sleep 100";

/// The two lines [`READS_TWO_LINES`] read.
async fn two_lines_read(dir: &Path) -> Vec<serde_json::Value> {
    let read = dir.join("lines");
    waiting::task::until(Duration::from_secs(10), "the script to read two lines", || read.exists()).await;
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
    agent.prompt(Seq::new(1), &Prompt::new("go")).await.unwrap();

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
    agent.prompt(Seq::new(1), &Prompt::new("go")).await.unwrap();

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

    let reported: Vec<ApprovalMode> = until_exit(&mut rx)
        .await
        .into_iter()
        .filter_map(|item| match item {
            AgentOutput::Event { body: EventBody::AgentMode { mode }, .. } => Some(mode),
            _ => None,
        })
        .collect();
    assert_eq!(reported, [fixtures::full_access(), fixtures::plan_mode()]);
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

/// What stands in for the CLI's answers to control requests: success to each, with nothing to say, but for
/// `get_settings`, which it answers with what `dir/settings.json` holds.
fn answering_with_settings(dir: &Path) -> String {
    let settings = dir.join("settings.json");
    format!(
        r#"while read -r line; do
id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
case "$line" in *'"get_settings"'*) said=$(cat '{}') ;; *) said=null ;; esac
[ -n "$id" ] && printf '{{"type":"control_response","response":{{"subtype":"success","request_id":"%s","response":%s}}}}\n' "$id" "$said"
done"#,
        settings.display()
    )
}

/// The CLI answers success to a level of effort it doesn't take, and changes nothing, so a change of effort has been
/// made only once the level it then reports in use is the one asked for.
#[tokio::test]
async fn an_effort_the_cli_says_it_took_and_doesnt_report_in_use_is_refused_as_not_applied() {
    let reported = [
        ("the level asked for", r#"{"applied":{"effort":"max"}}"#, None),
        ("another level", r#"{"applied":{"effort":"high"}}"#, Some(Some("high"))),
        ("no level", r#"{"applied":{}}"#, Some(None)),
    ];

    for (name, settings, refused) in reported {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), settings).unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let cli = script(dir.path(), &answering_with_settings(dir.path()));
        let agent = spawn_at(cli, spec(dir.path()), tx, AgentOptions::default()).await.unwrap();

        let max = Settings { effort: Some(Effort::Max), ..Settings::default() };
        let changed = agent.control(&protocol::capability::Control::Change(max)).await;

        let not_applied = match changed {
            Ok(()) => None,
            Err(AgentError::NotApplied { asked: "max", applied }) => Some(applied),
            Err(other) => panic!("{name}: {other:?}"),
        };
        assert_eq!(not_applied.as_ref().map(Option::as_deref), refused, "{name}");
        agent.stop().await;
    }
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

/// A prompt said out loud that the agent hasn't answered when its time is up gets a line said for it, which a CLI
/// started for the stand-in, on Haiku, writes. The script is both: the stand-in when it's started on Haiku, and
/// otherwise an agent that opens and then says nothing.
#[tokio::test]
async fn a_prompt_said_out_loud_and_not_answered_in_time_gets_a_line_said_for_it() {
    use protocol::capability::output::Covering;
    use protocol::capability::{Control, Report, prompts::Extras};
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
case "$*" in
*"--model haiku"*)
    read line
    printf '{"type":"result","subtype":"success","is_error":false,"result":"Hold on, one sec."}\n' ;;
*)
    printf '{"type":"system","subtype":"init","session_id":"s"}\n'
    exec cat >/dev/null ;;
esac"#;
    let (tx, mut rx) = mpsc::channel(64);
    let agent = spawn_at(script(dir.path(), body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();
    let prompt = Prompt { spoken: true, ..Prompt::new("How's it going?") };

    agent.control(&Control::Prompt { seq: Seq::new(3), prompt, extras: Extras::default() }).await.unwrap();

    let said = loop {
        let item = timeout(Duration::from_secs(20), rx.recv()).await.expect("a line said for it").expect("output");
        if let AgentOutput::Report(Report::Covering(said)) = item {
            break said;
        }
    };
    assert_eq!(said, Covering { prompt: Seq::new(3), line: "Hold on, one sec.".into() });
    agent.stop().await;
}
