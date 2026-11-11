use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::event::ContentBlock;
use protocol::model::RecentWork;
use serde_json::json;

use super::{CALLS_KEPT, RESULT_KEPT, TEXT_KEPT, add, start_of};

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

fn call(id: &str) -> ContentBlock {
    ContentBlock::ToolUse { id: id.into(), name: "Read".into(), input: json!({ "file_path": id }) }
}

fn result(id: &str, text: &str) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error: false,
        text: text.into(),
        images: Vec::new(),
        change: None,
        encrypted: false,
    }
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

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// What's kept of a text is its start, in whole characters, no more than `kept` of them, and cut exactly when the
    /// text had more.
    #[test]
    fn what_s_kept_is_the_text_s_start_and_says_whether_it_was_cut(text in "\\PC{0,40}", kept in 0usize..30) {
        let (start, cut) = start_of(&text, kept);

        prop_assert!(text.starts_with(&start));
        prop_assert_eq!(start.chars().count(), text.chars().count().min(kept));
        prop_assert_eq!(cut, text.chars().count() > kept);
    }
}
