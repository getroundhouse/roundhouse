use protocol::event::{Asking, CallKind, EventBody};
use protocol::fixtures;
use serde_json::{Value, json};

use super::{Recorded, ToolCall, asked_about, count};

/// A call recorded with the kind its adapter said, or with none, as one from before adapters said.
fn recorded<'a>(kind: Option<&'a CallKind>, name: &'a str, input: &'a Value) -> Recorded<'a> {
    Recorded { kind, name, input }
}

/// A request to use the tool `tool` with `input`, which says what it asks if `asking` does, and nothing of what it's
/// for.
fn request(tool: &str, input: Value, asking: Option<Asking>) -> EventBody {
    EventBody::ApprovalRequested {
        approval_id: "a".into(),
        tool_name: tool.into(),
        tool_use_id: None,
        input,
        description: None,
        suggestions: Value::Null,
        asking: asking.map(Box::new),
    }
}

/// The summary of a call recorded before adapters said its kind, which is read from its tool's name.
fn summary(name: &str, input: &Value) -> Option<String> {
    recorded(None, name, input).summary()
}

#[test]
fn a_known_tool_with_input_it_does_not_expect_is_drawn_like_any_other() {
    let input = json!({ "script": "ls" });

    let call = ToolCall::read(recorded(None, "Bash", &input));

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

    assert!(matches!(ToolCall::read(recorded(None, "Bash", &input)), ToolCall::Command(_)));
    assert!(matches!(ToolCall::read(recorded(None, "mcp__shell__run", &input)), ToolCall::Other(_)));
}

#[test]
fn the_advisor_is_known_by_its_tool() {
    assert!(matches!(ToolCall::read(recorded(None, "advisor", &json!({}))), ToolCall::Advisor));
    assert!(matches!(ToolCall::read(recorded(None, "mcp__team__advisor", &json!({}))), ToolCall::Other(_)));
}

/// So a client needs no agent's name for a tool: whatever the tool is called, the call is what its adapter said.
#[test]
fn a_call_whose_adapter_said_its_kind_is_read_by_that_whatever_its_tool_is_called() {
    let input = json!({ "command": "ls", "description": "List files" });

    assert!(matches!(ToolCall::read(recorded(Some(&CallKind::Command), "shell", &input)), ToolCall::Command(_)));
    let said_other = CallKind::Other { server: None, tool: "Bash".into() };
    assert!(
        matches!(ToolCall::read(recorded(Some(&said_other), "Bash", &input)), ToolCall::Other(_)),
        "not by its name"
    );
    let search = json!({ "pattern": "*.rs" });
    let found = ToolCall::read(recorded(Some(&CallKind::FindFiles), "find", &search));
    assert!(matches!(found, ToolCall::Search(search) if search.looks == super::Looks::ForNames));
    assert!(matches!(ToolCall::read(recorded(Some(&CallKind::Advisor), "consult", &json!({}))), ToolCall::Advisor));
    assert!(matches!(ToolCall::read(recorded(Some(&CallKind::Subagent), "delegate", &json!({}))), ToolCall::Agent(_)));
}

#[test]
fn a_call_of_a_kind_this_build_doesnt_know_is_drawn_as_any_tool_by_its_name() {
    let input = json!({ "command": "ls" });

    let call = ToolCall::read(recorded(Some(&CallKind::Unknown), "Bash", &input));

    let ToolCall::Other(other) = call else { panic!("its kind isn't one this build draws") };
    assert_eq!((other.title.as_str(), other.detail.as_deref()), ("Bash", Some("ls")));
}

#[test]
fn what_a_call_does_is_its_kind_or_for_one_from_before_kinds_its_tool_s() {
    let nothing = Value::Null;
    let hands_off = |kind, name| recorded(kind, name, &nothing).hands_off();
    let asks_the_person = |kind, name| recorded(kind, name, &nothing).asks_the_person();

    assert!(hands_off(Some(&CallKind::Subagent), "delegate") && hands_off(None, "Agent") && hands_off(None, "Task"));
    assert!(!hands_off(Some(&CallKind::Command), "Agent"), "what its adapter said, not its name");
    assert!(asks_the_person(Some(&CallKind::Questions), "ask") && asks_the_person(Some(&CallKind::Plan), "plan"));
    assert!(asks_the_person(None, "AskUserQuestion") && asks_the_person(None, "ExitPlanMode"));
    assert!(!asks_the_person(None, "Bash") && !asks_the_person(Some(&CallKind::Command), "ExitPlanMode"));
}

/// A call is made only from its block, or the request to make it, which is what keeps the kind its adapter said with
/// it.
#[test]
fn a_call_is_read_from_its_block_and_from_a_request_to_make_it_with_the_kind_each_says() {
    let before = fixtures::tool_use("c", "shell", json!({ "command": "ls" }));
    let since = fixtures::of_kind(before.clone(), CallKind::Command);
    assert!(matches!(ToolCall::read(Recorded::of(&before).unwrap()), ToolCall::Other(_)), "no kind, and no such tool");
    assert!(matches!(ToolCall::read(Recorded::of(&since).unwrap()), ToolCall::Command(_)));
    assert!(Recorded::of(&fixtures::tool_result("c", "a.rs", false)).is_none(), "a result is no call");

    let request = |asking| request("shell", json!({ "command": "ls -la" }), asking);
    let said = request(Some(Asking::ToolUse { call: Some(CallKind::Command) }));
    assert!(matches!(ToolCall::read(Recorded::asked(&said).unwrap()), ToolCall::Command(_)));
    assert_eq!(asked_about(&said).as_deref(), Some("ls -la"));
    let before = request(None);
    assert!(matches!(ToolCall::read(Recorded::asked(&before).unwrap()), ToolCall::Other(_)));
    assert!(Recorded::asked(&EventBody::Unknown).is_none() && asked_about(&EventBody::Unknown).is_none());
}

/// The tools here have names Claude gave other tools, so a request read by its tool's name would read as those. Only
/// the last, a tool's use that says no kind, is one recorded before adapters said, and is read by its name.
#[test]
fn a_request_that_says_its_kind_is_never_read_by_its_tool_s_name() {
    let asking = |tool: &str, asking| request(tool, json!({ "pattern": "fn main", "command": "ls" }), Some(asking));
    let questions = asking("Grep", Asking::Questions { questions: Vec::new() });
    let plan = asking("Bash", Asking::Plan { plan: String::new() });
    let newer = asking("Bash", Asking::Unknown);
    let no_kind = asking("Bash", Asking::ToolUse { call: None });

    assert!(Recorded::asked(&questions).unwrap().asks_the_person());
    assert!(Recorded::asked(&plan).unwrap().asks_the_person());
    assert!(matches!(ToolCall::read(Recorded::asked(&newer).unwrap()), ToolCall::Other(_)), "any tool, not a command");
    assert!(matches!(ToolCall::read(Recorded::asked(&no_kind).unwrap()), ToolCall::Command(_)), "by its tool's name");
}

#[test]
fn a_request_is_about_what_the_agent_said_it_s_for_when_it_said() {
    let request = EventBody::ApprovalRequested {
        approval_id: "a".into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: json!({ "command": "rm -rf target" }),
        description: Some("Clear the build".into()),
        suggestions: Value::Null,
        asking: None,
    };

    assert_eq!(asked_about(&request).as_deref(), Some("Clear the build"));
}
