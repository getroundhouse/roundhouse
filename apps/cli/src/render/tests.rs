use client_core::calls::{Failure, Recorded};
use protocol::event::{AgentTask, CallKind, ContentBlock, Denied, EndedTask, TaskKind, TaskOutcome};
use protocol::fixtures;
use protocol::model::{EndedWait, Seq, WaitEnd, WaitEnded};
use serde_json::{Value, json};

use super::{Printer, called, came_back, denial, own_turn, started};

fn task(id: &str, kind: TaskKind) -> AgentTask {
    AgentTask { id: id.into(), description: Some(format!("Do {id}")), kind, call: None }
}

#[test]
fn a_list_of_background_tasks_said_again_with_a_kind_changed_prints_once() {
    let mut printer = Printer::default();
    let listed = [task("a", TaskKind::Command), task("b", TaskKind::Command)];
    let again = [task("a", TaskKind::Command), task("b", TaskKind::Monitor)];

    assert_eq!(printer.newly_running(&listed).as_deref(), Some("running in the background: Do a, Do b"));
    assert_eq!(printer.newly_running(&again), None);
    assert_eq!(printer.newly_running(&listed[..1]).as_deref(), Some("running in the background: Do a"));
    assert_eq!(printer.newly_running(&[]).as_deref(), Some("running in the background: nothing"));
    assert_eq!(printer.newly_running(&listed[..1]).as_deref(), Some("running in the background: Do a"));
}

#[test]
fn a_call_says_what_it_s_about_as_every_client_does() {
    let said = |name: &str, input: Value| called(Recorded::of(&fixtures::tool_use("c", name, input)).unwrap());

    assert_eq!(said("Bash", json!({ "command": "\ncargo test\n--workspace" })), "Bash: cargo test");
    assert_eq!(said("Read", json!({ "file_path": "src/a.rs" })), "Read: src/a.rs");
    assert_eq!(said("mcp__github__list", json!({ "labels": ["bug"] })), "mcp__github__list: labels: [\"bug\"]");
    assert_eq!(said("advisor", json!({})), "advisor", "it takes no input");
}

/// So the terminal reads a call as the app does: by the kind its adapter said, whatever the tool is called.
#[test]
fn a_call_is_said_by_the_kind_its_adapter_said_it_is() {
    let before = fixtures::tool_use("c", "seek", json!({ "path": "src", "pattern": "fn main" }));
    let since = fixtures::of_kind(before.clone(), CallKind::Search);

    assert_eq!(called(Recorded::of(&before).unwrap()), "seek: src", "any tool, by the first of its usual inputs");
    assert_eq!(called(Recorded::of(&since).unwrap()), "seek: fn main", "a search, by what it looks for");
}

#[test]
fn a_failed_result_says_how_the_command_exited_however_it_was_recorded() {
    fn said(block: &ContentBlock) -> (String, &str) {
        let ContentBlock::ToolResult { text, .. } = block else { panic!("a result") };
        came_back(text, Failure::of(block))
    }
    let before = fixtures::tool_result("c", "Exit code 2\nls: x: No such file", true);
    let since = fixtures::command_failed("c", "ls: x: No such file", 2);

    for block in [&before, &since] {
        assert_eq!(said(block), ("tool error, exit 2".to_string(), "ls: x: No such file"));
    }
    let refused = fixtures::tool_result("c", "<tool_use_error>no such tool</tool_use_error>", true);
    assert_eq!(said(&refused), ("tool error".to_string(), "no such tool"));
    let worked = fixtures::tool_result("c", "a.rs", false);
    assert_eq!(said(&worked), ("tool result".to_string(), "a.rs"));
}

#[test]
fn an_agent_started_on_another_build_than_its_chat_s_last_says_it_was_updated() {
    let said = "agent resumed, pid 7, Roundhouse chat worker updated from 0.9.0 to 1.0.0";
    assert_eq!(started(7, true, Some("1.0.0"), Some("0.9.0")), said);
    assert_eq!(started(7, false, Some("1.0.0"), None), "agent started, pid 7");
    assert_eq!(started(7, true, None, None), "agent resumed, pid 7", "from a hub from before builds were recorded");
}

#[test]
fn a_subagents_end_says_which_it_was() {
    let mut printer = Printer::default();
    printer.subagent(Seq::new(1), Some("Find the tests"), &"toolu_a".into());
    printer.subagent(Seq::new(2), None, &"toolu_b".into());

    assert_eq!(printer.ending(&"toolu_a".into(), TaskOutcome::Finished), "a subagent finished: Find the tests");
    assert_eq!(printer.ending(&"toolu_b".into(), TaskOutcome::Stopped), "a subagent stopped: toolu_b");
    assert_eq!(
        printer.ending(&"toolu_c".into(), TaskOutcome::Failed),
        "a subagent failed: toolu_c",
        "one it didn't see start"
    );
}

#[test]
fn a_denied_call_says_so_with_what_the_person_said_rather_than_failing() {
    let said = Denied { said: Some("Use the full path\nand say so".into()) };

    assert_eq!(denial(&said), "tool denied: Use the full path");
    assert_eq!(denial(&Denied::default()), "tool denied");
}

#[test]
fn a_turn_the_agent_started_by_itself_says_which_tasks_it_heard_had_ended() {
    let ended = |id: &str, description: Option<&str>, outcome| EndedTask {
        task: AgentTask {
            id: id.into(),
            description: description.map(String::from),
            kind: TaskKind::Command,
            call: None,
        },
        outcome,
        wait: None,
    };

    let said =
        own_turn(&[ended("b1", Some("Run the tests"), TaskOutcome::Failed), ended("b2", None, TaskOutcome::Finished)]);

    assert_eq!(
        said, "the agent picked up by itself, after: Run the tests failed; b2 finished",
        "by its ID with no name"
    );
    let mut wait = ended("k3", Some("roundhouse/wait_for_pull_requests"), TaskOutcome::Failed);
    wait.task.kind = TaskKind::Wait;
    let unrecorded = "the agent picked up by itself, after: waited on a pull request (failed)";
    assert_eq!(own_turn(std::slice::from_ref(&wait)), unrecorded, "not by its tool's name");
    let mut kept = fixtures::pull_request_wait("k3");
    kept.ended = Some(WaitEnded { at_ms: 2, how: WaitEnd::CalledOffByPerson });
    wait.wait = Some(EndedWait::from(&kept));
    let as_the_app_says = "the agent picked up by itself, after: waited on #7: required checks (Stopped)";
    assert_eq!(own_turn(&[wait]), as_the_app_says, "from the hub's record, not the agent's word");
    assert_eq!(own_turn(&[]), "the agent picked up by itself");
}
