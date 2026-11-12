use serde_json::{Value, json};

use super::{ToolCall, count};

fn summary(name: &str, input: &Value) -> Option<String> {
    ToolCall::read(name, input).summary()
}

#[test]
fn a_known_tool_with_input_it_does_not_expect_is_drawn_like_any_other() {
    let input = json!({ "script": "ls" });

    let call = ToolCall::read("Bash", &input);

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

    assert!(matches!(ToolCall::read("Bash", &input), ToolCall::Command(_)));
    assert!(matches!(ToolCall::read("mcp__shell__run", &input), ToolCall::Other(_)));
}

#[test]
fn the_advisor_is_known_by_its_tool() {
    assert!(matches!(ToolCall::read("advisor", &json!({})), ToolCall::Advisor));
    assert!(matches!(ToolCall::read("mcp__team__advisor", &json!({})), ToolCall::Other(_)));
}
