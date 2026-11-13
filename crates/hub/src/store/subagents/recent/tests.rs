use protocol::event::{CallKind, ContentBlock, Denied};
use protocol::fixtures;
use protocol::model::RecentWork;
use serde_json::json;

use super::{CALLS_KEPT, RESULT_KEPT, TEXT_KEPT, add};

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

fn call(id: &str) -> ContentBlock {
    fixtures::tool_use(id, "Read", json!({ "file_path": id }))
}

fn result(id: &str, text: &str) -> ContentBlock {
    fixtures::tool_result(id, text, false)
}

/// Each call's id, and the text kept just before it.
fn calls(work: &RecentWork) -> Vec<(&str, Option<&str>)> {
    work.calls.iter().map(|made| (made.call.id.as_str(), made.before.as_ref().map(|said| said.text.as_str()))).collect()
}

#[test]
fn the_newest_calls_are_kept_each_with_the_text_just_before_it_then_what_it_has_said_since() {
    let mut work = RecentWork::default();

    assert!(add(&mut work, &[text("First I'll look."), call("a")], true));
    assert!(add(&mut work, &[call("b")], true));
    assert!(add(&mut work, &[text("Then this."), text("And that."), call("c"), call("d")], true));
    assert!(add(&mut work, &[text("Nearly done.")], true));

    assert_eq!(CALLS_KEPT, 3);
    assert_eq!(calls(&work), [("b", None), ("c", Some("Then this.\n\nAnd that.")), ("d", None)], "the oldest went");
    assert_eq!(work.since.as_ref().map(|said| said.text.as_str()), Some("Nearly done."));
    assert!(!add(&mut work, &[text("  "), ContentBlock::Thinking { text: "Hm.".into() }], true), "nothing to show");
}

#[test]
fn a_call_s_result_is_kept_with_it_cut_short() {
    let mut work = RecentWork::default();
    add(&mut work, &[call("short"), call("long")], true);
    let long = (0..RESULT_KEPT).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");

    assert!(add(&mut work, &[result("short", "one\ntwo"), result("long", &long)], true));
    assert!(!add(&mut work, &[result("gone", "x")], true), "a call it doesn't keep");

    let short = work.calls[0].result.as_ref().unwrap();
    assert_eq!((short.text.as_str(), short.cut), ("one\ntwo", false));
    let long = work.calls[1].result.as_ref().unwrap();
    assert_eq!((long.text.chars().count(), long.cut), (RESULT_KEPT, true));
}

#[test]
fn a_long_text_keeps_its_start_whole_characters_and_all() {
    let mut work = RecentWork::default();
    let long = "한".repeat(TEXT_KEPT + 1);

    add(&mut work, &[text(&long)], true);
    assert!(!add(&mut work, &[text("More.")], true), "more of a text that's cut changes nothing, so sends no notice");

    let since = work.since.as_ref().unwrap();
    assert_eq!((since.text.chars().count(), since.cut), (TEXT_KEPT, true));
    assert!(since.text.chars().all(|c| c == '한'), "nothing more is added to a text that's cut");
}

/// Text in a `user` line was written to the subagent, such as the prompt the CLI sends one run in the foreground, so
/// only what came back is taken from one.
#[test]
fn only_what_came_back_is_taken_from_a_user_line() {
    let mut work = RecentWork::default();
    add(&mut work, &[call("a")], true);

    assert!(!add(&mut work, &[text("Find where the tests live.")], false), "its prompt");
    assert!(add(&mut work, &[result("a", "one")], false));

    assert_eq!(work.since, None);
    assert_eq!(work.calls[0].result.as_ref().map(|result| result.text.as_str()), Some("one"));
}

/// So a client draws a subagent's call from its record as it would from its events, by the kind its adapter said, and
/// says how a failed command exited without that being in the text.
#[test]
fn a_call_keeps_the_kind_its_adapter_said_and_its_result_how_the_command_exited() {
    let mut work = RecentWork::default();
    let said = Some(CallKind::Command);
    let made = fixtures::of_kind(fixtures::tool_use("c", "shell", json!({ "command": "ls x" })), CallKind::Command);
    let failed = fixtures::command_failed("c", "ls: x: No such file", 2);

    add(&mut work, &[made, call("older")], true);
    add(&mut work, &[failed], true);

    assert_eq!((&work.calls[0].call.kind, &work.calls[1].call.kind), (&said, &None));
    let result = work.calls[0].result.as_ref().unwrap();
    assert_eq!((result.exit_code, result.text.as_str()), (Some(2), "ls: x: No such file"));
}

/// So a client says a subagent's denied call was denied, with what the person said, from its record.
#[test]
fn a_denied_calls_result_says_so() {
    let mut work = RecentWork::default();
    let made = fixtures::tool_use("c", "Bash", json!({ "command": "rm -rf x" }));

    add(&mut work, &[made], true);
    add(&mut work, &[fixtures::tool_denied("c", "Not that", Some("Not that"))], true);

    assert_eq!(work.calls[0].result.as_ref().unwrap().denied, Some(Denied { said: Some("Not that".into()) }));
}
