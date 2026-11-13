use client_core::calls::{Failure, Recorded};
use protocol::event::{AgentTask, CallKind, ContentBlock, Denied, SubagentOutcome, TaskKind};
use protocol::fixtures;
use protocol::model::Seq;
use serde_json::{Value, json};

use super::{Printer, called, came_back, denial, started};

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

    assert_eq!(printer.ending(&"toolu_a".into(), SubagentOutcome::Finished), "a subagent finished: Find the tests");
    assert_eq!(printer.ending(&"toolu_b".into(), SubagentOutcome::Stopped), "a subagent stopped: toolu_b");
    assert_eq!(
        printer.ending(&"toolu_c".into(), SubagentOutcome::Failed),
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
