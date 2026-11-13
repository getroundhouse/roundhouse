use protocol::event::CallKind;
use serde_json::{Value, json};

use super::{ToolCall, asks_the_person, count, hands_off};

/// The summary of a call recorded before adapters said its kind, which is read from its tool's name.
fn summary(name: &str, input: &Value) -> Option<String> {
    ToolCall::read(None, name, input).summary()
}

#[test]
fn a_known_tool_with_input_it_does_not_expect_is_drawn_like_any_other() {
    let input = json!({ "script": "ls" });

    let call = ToolCall::read(None, "Bash", &input);

    let ToolCall::Other(other) = call else { panic!("a command needs its command") };
    assert_eq!(other.title, "Bash");
}

#[test]
fn a_call_s_summary_is_the_input_that_matters_most_for_its_kind() {
    let command = json!({ "command": "\ncargo test\nsecond line", "description": "Run the tests" });
    assert_eq!(summary("Bash", &command).as_deref(), Some("cargo test"), "its first line with anything on it");
    let edit = json!({ "file_path": "src/a.rs", "old_string": "y", "new_string": "z" });
    assert_eq!(summary("Edit", &edit).as_deref(), Some("src/a.rs"));
    let search = json!({ "path": "src", "pattern": "fn main" });
    assert_eq!(summary("Grep", &search).as_deref(), Some("fn main"), "what it looks for, rather than where");
    let fetch = json!({ "url": "https://docs.rs/serde", "prompt": "What derives are there?" });
    assert_eq!(summary("WebFetch", &fetch).as_deref(), Some("https://docs.rs/serde"));
    assert_eq!(summary("WebSearch", &json!({ "query": "rail signals" })).as_deref(), Some("rail signals"));
    assert_eq!(summary("advisor", &json!({})), None, "it takes no input");
}

#[test]
fn a_subagent_s_summary_is_what_it_s_for_or_else_what_it_was_asked() {
    let described = json!({ "description": "Find the tests", "prompt": "Look for\nthe tests" });
    let asked = json!({ "prompt": "Look for\nthe tests" });

    assert_eq!(summary("Agent", &described).as_deref(), Some("Find the tests"));
    assert_eq!(summary("Agent", &asked).as_deref(), Some("Look for"));
    assert_eq!(summary("Agent", &json!({})), None);
}

#[test]
fn any_other_tool_s_summary_is_its_usual_input_or_else_its_first() {
    assert_eq!(
        summary("mcp__github__create_issue", &json!({ "title": "Signals" })).as_deref(),
        Some("title: \"Signals\"")
    );
    assert_eq!(summary("ToolSearch", &json!({ "query": "select:Read\nmore" })).as_deref(), Some("select:Read"));
    let described = json!({ "description": " Tidy up ", "todos": [] });
    assert_eq!(summary("TodoWrite", &described).as_deref(), Some("Tidy up"), "what it's for says more");
    assert_eq!(summary("TodoWrite", &json!({ "todos": [] })).as_deref(), Some("todos: []"));
    assert_eq!(summary("Ping", &json!({})), None);
}

#[test]
fn a_count_says_one_or_many() {
    assert_eq!(count(1, "line", "lines"), "1 line");
    assert_eq!(count(0, "line", "lines"), "0 lines");
    assert_eq!(count(12, "match", "matches"), "12 matches");
}

#[test]
fn a_command_is_known_by_its_tool() {
    let input = json!({ "command": "ls", "description": "List files" });

    assert!(matches!(ToolCall::read(None, "Bash", &input), ToolCall::Command(_)));
    assert!(matches!(ToolCall::read(None, "mcp__shell__run", &input), ToolCall::Other(_)));
}

#[test]
fn the_advisor_is_known_by_its_tool() {
    assert!(matches!(ToolCall::read(None, "advisor", &json!({})), ToolCall::Advisor));
    assert!(matches!(ToolCall::read(None, "mcp__team__advisor", &json!({})), ToolCall::Other(_)));
}

/// So a client needs no agent's name for a tool: whatever the tool is called, the call is what its adapter said.
#[test]
fn a_call_whose_adapter_said_its_kind_is_read_by_that_whatever_its_tool_is_called() {
    let input = json!({ "command": "ls", "description": "List files" });

    assert!(matches!(ToolCall::read(Some(&CallKind::Command), "shell", &input), ToolCall::Command(_)));
    let said_other = CallKind::Other { server: None, tool: "Bash".into() };
    assert!(matches!(ToolCall::read(Some(&said_other), "Bash", &input), ToolCall::Other(_)), "not by its name");
    let search = json!({ "pattern": "*.rs" });
    let found = ToolCall::read(Some(&CallKind::FindFiles), "find", &search);
    assert!(matches!(found, ToolCall::Search(search) if search.looks == super::Looks::ForNames));
    assert!(matches!(ToolCall::read(Some(&CallKind::Advisor), "consult", &json!({})), ToolCall::Advisor));
    assert!(matches!(ToolCall::read(Some(&CallKind::Subagent), "delegate", &json!({})), ToolCall::Agent(_)));
}

#[test]
fn a_call_of_a_kind_this_build_doesnt_know_is_drawn_as_any_tool_by_its_name() {
    let input = json!({ "command": "ls" });

    let call = ToolCall::read(Some(&CallKind::Unknown), "Bash", &input);

    let ToolCall::Other(other) = call else { panic!("its kind isn't one this build draws") };
    assert_eq!((other.title.as_str(), other.detail.as_deref()), ("Bash", Some("ls")));
}

#[test]
fn what_a_call_does_is_its_kind_or_for_one_from_before_kinds_its_tool_s() {
    assert!(hands_off(Some(&CallKind::Subagent), "delegate") && hands_off(None, "Agent") && hands_off(None, "Task"));
    assert!(!hands_off(Some(&CallKind::Command), "Agent"), "what its adapter said, not its name");
    assert!(asks_the_person(Some(&CallKind::Questions), "ask") && asks_the_person(Some(&CallKind::Plan), "plan"));
    assert!(asks_the_person(None, "AskUserQuestion") && asks_the_person(None, "ExitPlanMode"));
    assert!(!asks_the_person(None, "Bash") && !asks_the_person(Some(&CallKind::Command), "ExitPlanMode"));
}
