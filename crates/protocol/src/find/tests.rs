use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use serde_json::json;

use super::*;
use crate::event::{Hunk, HunkLine, LineMark};
use crate::fixtures;

fn needle(query: &str) -> Needle {
    Needle::new(query).unwrap()
}

fn found<'a>(query: &str, text: &'a str) -> Vec<&'a str> {
    needle(query).ranges(text).map(|range| &text[range]).collect()
}

#[test]
fn case_is_ignored_and_matches_dont_overlap() {
    assert_eq!(found("fix", "Fix the FIX, then fix it"), ["Fix", "FIX", "fix"]);
    assert_eq!(found("aa", "aaaa a"), ["aa", "aa"], "each looked for after the last");
    assert_eq!(found("é", "CAFÉ café"), ["É", "é"]);
    assert!(found("x", "nothing here").is_empty());
}

#[test]
fn a_character_whose_lower_case_is_two_matches_only_both() {
    // `İ` is `i` and a combining dot in lower case.
    assert_eq!(found("i\u{307}", "İ"), ["İ"]);
    assert!(found("i", "İ").is_empty(), "a match never stops partway through a character");
}

#[test]
fn a_needle_is_something_to_look_for_and_not_too_much() {
    assert_eq!(Needle::new(""), Err(NeedleError::Empty));
    assert!(Needle::new(" ").is_ok(), "a space is something to look for");
    assert!(Needle::new(&"x".repeat(Needle::MOST_CHARS)).is_ok());
    assert_eq!(Needle::new(&"é".repeat(Needle::MOST_CHARS + 1)), Err(NeedleError::TooLong), "counted in characters");
}

fn call(input: Value) -> EventBody {
    let block = fixtures::tool_use("t", "Bash", input);
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: None,
        model: None,
        blocks: vec![block],
        first_index: None,
    }
}

fn changed(text: &str, line: &str) -> EventBody {
    let hunk = Hunk { old_start: 1, new_start: 1, lines: vec![HunkLine { mark: LineMark::Added, text: line.into() }] };
    let mut block = fixtures::tool_result("t", text, false);
    if let ContentBlock::ToolResult { change, .. } = &mut block {
        *change = Some(FileChange::Changed { hunks: vec![hunk] });
    }
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
}

#[test]
fn an_event_holds_what_it_records_of_a_call_and_its_result() {
    let call = call(json!({ "command": "cargo test", "timeout": 600, "env": { "RUST_LOG": "debug" } }));
    let result = changed("The file was updated", "fn slow_down()");

    assert!(needle("CARGO").is_in_event(&call));
    assert!(needle("rust_log").is_in_event(&call), "a key, as the JSON shown has it");
    assert!(needle("\"timeout\": 600").is_in_event(&call), "and the JSON itself");
    assert!(needle("slow_down").is_in_event(&result), "a change's lines");
    assert!(!needle("Bash").is_in_event(&call), "the tool's name is the app's to show");
    assert!(!needle("granted").is_in_event(&EventBody::PromptTaken { prompt: 1 }));
}

#[test]
fn json_that_cant_hold_a_needle_is_passed_over() {
    let body = serde_json::to_string(&call(json!({ "command": "say \"hi\"\n" }))).unwrap();

    assert!(!needle("missing").may_be_in_json(&body));
    assert!(needle("SAY").may_be_in_json(&body));
    assert!(needle("\"hi\"").may_be_in_json(&body), "a quote, which JSON writes another way");
    assert!(needle(" 600").may_be_in_json(&body), "nor a space where the JSON shown has one");
}

#[test]
fn a_phrase_across_a_line_is_found_as_a_paragraph_shows_it() {
    let says = |text: &str| EventBody::Notice { level: crate::event::NoticeLevel::Info, message: text.into() };
    let wrapped = says("The parser\nis fixed");

    assert!(needle("parser is").is_in_event(&wrapped));
    assert!(needle("parser is").may_be_in_json(&serde_json::to_string(&wrapped).unwrap()), "as JSON writes it too");
    assert!(needle("parser  is").may_be_in_markdown("The parser is"), "however much whitespace either has");
}

#[test]
fn markdown_is_found_past_what_marks_it() {
    let says = |text: &str| EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: None,
        model: None,
        blocks: vec![ContentBlock::Text { text: text.into(), images: Vec::new() }],
        first_index: None,
    };

    assert!(needle("parser is").may_be_in_markdown("The **parser** is fixed"));
    assert!(needle("run cargo test").may_be_in_markdown("Run `cargo test` now"));
    assert!(needle("snake_case").may_be_in_markdown("a `snake_case` name"), "with the needle's marks too");
    assert!(!needle("parser was").may_be_in_markdown("The **parser** is fixed"));
    assert!(needle("parser is").is_in_event(&says("The _parser_ is")));
    let body = serde_json::to_string(&says("The ~~parser~~ is")).unwrap();
    assert!(needle("parser is").may_be_in_json(&body), "and in what the hub keeps");
}

#[test]
fn a_needle_of_marks_alone_is_looked_for_as_it_is() {
    assert!(needle("**").may_be_in_markdown("a **b**"));
    assert!(!needle("**").may_be_in_markdown("plain"));
}

/// Text that has what matching gets wrong: letters of either case, ones whose lower case is longer or not ASCII, what
/// Markdown marks text with, and what's around them.
fn text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        "[aAbB ]{0,6}",
        "[*_`~\\\\]{1,2}",
        "[\n\t \"\\\\:,{}]{1,2}",
        Just("é".to_string()),
        Just("É".to_string()),
        Just("İ".to_string()),
        Just("ß".to_string()),
        Just("\u{212A}".to_string()),
        "\\PC{0,3}",
    ];
    proptest::collection::vec(piece, 0..10).prop_map(|pieces| pieces.concat())
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// Every match is the needle in lower case, between characters, in order and apart, and none is missed between
    /// them.
    #[test]
    fn matches_are_the_needle_ignoring_case_and_none_is_missed(text in text(), query in text()) {
        let Ok(needle) = Needle::new(&query) else { return Ok(()) };
        let lower = |text: &str| text.chars().flat_map(char::to_lowercase).collect::<String>();
        let ranges: Vec<Range<usize>> = needle.ranges(&text).collect();

        let mut from = 0;
        for range in &ranges {
            prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            prop_assert!(range.start >= from, "{ranges:?} overlap");
            prop_assert_eq!(lower(&text[range.clone()]), lower(&query));
            let skipped = &text[from..range.start];
            prop_assert!(skipped.char_indices().all(|(at, _)| needle.matches_at(&text[from + at..]).is_none()));
            from = range.end;
        }
        prop_assert!(text[from..].char_indices().all(|(at, _)| needle.matches_at(&text[from + at..]).is_none()));
    }

    /// Looking past marks finds anything that's there as it is, so the hub never passes over what a client finds.
    #[test]
    fn looking_past_marks_misses_nothing_that_s_there(text in text(), query in text()) {
        let Ok(needle) = Needle::new(&query) else { return Ok(()) };

        prop_assert!(!needle.is_in(&text) || needle.may_be_in_markdown(&text));
    }

    /// What the hub passes over in what it keeps never holds what it would find in the event, so it names every event
    /// a client could find a match in.
    #[test]
    fn the_hub_passes_over_only_what_can_t_hold_the_needle(
        texts in proptest::collection::vec(text(), 4),
        query in text(),
        cut in (0..4usize, any::<proptest::sample::Index>(), any::<proptest::sample::Index>()),
        typed in any::<bool>(),
    ) {
        // Half the time, part of what an event says, typed as it shows: a line break as a space.
        let (which, start, len) = cut;
        let chars: Vec<char> = texts[which].chars().collect();
        let from = start.index(chars.len() + 1);
        let part: String = chars[from..from + len.index(chars.len() - from + 1)].iter().collect();
        let query = if typed { part.replace(['\n', '\t'], " ") } else { query };
        let Ok(needle) = Needle::new(&query) else { return Ok(()) };
        let said = ContentBlock::Text { text: texts[0].clone(), images: Vec::new() };
        let input = json!({ "command": texts[1], "timeout": 600, "list": [texts[2]] });
        let used = fixtures::tool_use("t", "Bash", input);
        let blocks = vec![said, used];
        let bodies = [
            EventBody::UserPrompt { client_msg_id: None, prompt: crate::prompt::Prompt::new(texts[3].clone()) },
            EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None },
            changed(&texts[0], &texts[1]),
        ];

        for body in &bodies {
            let json = serde_json::to_string(body).unwrap();
            prop_assert!(!needle.is_in_event(body) || needle.may_be_in_json(&json), "{query:?} in {json}");
        }
    }

    /// The quick way for ASCII finds what the slow way does.
    #[test]
    fn ascii_is_found_as_any_text_is(text in "[aAbB -]{0,24}", query in "[aAbB -]{1,4}") {
        let needle = Needle::new(&query).unwrap();
        let quick: Vec<Range<usize>> = needle.ranges(&text).collect();
        let mut slow = Vec::new();
        let mut from = 0;
        while let Some(found) = needle.next(&text, from) {
            from = found.end;
            slow.push(found);
        }

        prop_assert_eq!(quick, slow);
    }
}
