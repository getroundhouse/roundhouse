use client_core::session_log::SessionLog;
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, TurnOutcome};
use serde_json::json;

use protocol::find::Needle;

use super::{count, of_row, rows_drawing};
use crate::find::Part;
use crate::fixtures::{calls_to, event, from_the_agent, log, within};
use crate::folds::Fold;
use crate::transcript::Model;

fn prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { text: text.into(), client_msg_id: None, attachments: Vec::new() }
}

fn agent(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

fn says(text: &str) -> EventBody {
    agent(vec![ContentBlock::Text { text: text.into(), images: Vec::new() }])
}

fn call(id: &str, name: &str, input: serde_json::Value) -> EventBody {
    agent(vec![ContentBlock::ToolUse { id: id.into(), name: name.into(), input }])
}

fn sent_back(id: &str, text: &str) -> EventBody {
    let block = ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error: false,
        text: text.into(),
        images: Vec::new(),
        change: None,
        encrypted: false,
    };
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
}

#[test]
fn what_the_person_and_the_agent_wrote_is_found_whatever_its_case() {
    let log = log([
        event(1, prompt("Fix the Parser")),
        event(2, EventBody::PromptTaken { prompt: 1 }),
        event(3, says("The **parser** is fixed, and PARSER tests pass.")),
        event(4, agent(vec![ContentBlock::Thinking { text: "The parser was slow".into() }])),
    ]);

    assert_eq!(count(&log, "parser", 1), 1, "the prompt, where the agent took it");
    assert_eq!(count(&log, "parser", 2), 2, "in the words, not the Markdown around them");
    assert_eq!(count(&log, "parser", 3), 1, "thinking too, closed or not");
    assert_eq!(count(&log, "parser is", 2), 1, "however it's styled");
}

/// A prompt sent mid-turn is drawn where the agent took it, after the thought it was writing, and at the end of the
/// chat until then, where nothing is found.
#[test]
fn a_prompt_is_found_where_its_drawn() {
    let mut log = log([
        event(1, prompt("Start")),
        event(2, EventBody::PromptTaken { prompt: 1 }),
        event(3, prompt("Only the sidebar")),
        event(4, agent(vec![ContentBlock::Thinking { text: "The sidebar first".into() }])),
    ]);
    let needle = |log: &SessionLog| (0..5).map(|row| count(log, "sidebar", row)).collect::<Vec<_>>();

    assert_eq!(needle(&log), [0, 0, 0, 1, 0], "the thought, and not the prompt waiting at the end");
    assert_eq!(rows_drawing(&log, &Model::default(), 3), Vec::<usize>::new());
    log.apply_event(event(5, EventBody::PromptTaken { prompt: 3 }));
    assert_eq!(needle(&log), [0, 0, 0, 1, 1], "under the thought, in the row that took it");
    assert_eq!(rows_drawing(&log, &Model::default(), 3), [4]);
}

#[test]
fn a_call_s_result_is_found_in_the_call_s_row_which_draws_it() {
    let log = log([
        event(1, call("run", "Bash", json!({ "command": "cargo test", "description": "Run the tests" }))),
        event(2, sent_back("run", "test parser ... FAILED\ntest lexer ... ok")),
    ]);

    assert_eq!(count(&log, "failed", 0), 1, "what it printed");
    assert_eq!(count(&log, "cargo", 0), 1, "the command");
    assert_eq!(count(&log, "the tests", 0), 1, "what the agent said it's for");
    assert_eq!(count(&log, "failed", 1), 0, "the result draws nothing of its own");
    assert_eq!(rows_drawing(&log, &Model::default(), 2), [0, 1], "but its call's row is counted again");
}

#[test]
fn what_the_app_writes_itself_isn_t_found() {
    let log = log([
        event(1, call("read", "Read", json!({ "file_path": "/src/main.rs" }))),
        event(2, sent_back("read", "     1\tfn main() {}\n     2\t")),
    ]);

    assert_eq!(count(&log, "main", 0), 2, "the path over the lines, and the line, once it's open");
    assert_eq!(count(&log, "Read", 0), 0, "the line's label");
    assert_eq!(count(&log, "2 lines", 0), 0, "how much it read");
    assert_eq!(count(&log, "1", 0), 0, "the lines' numbers");
}

#[test]
fn a_subagent_s_work_is_found_in_the_call_that_started_it() {
    let log =
        log([event(1, calls_to("Agent", &["helper"])), event(2, within("helper", says("Looking at the parser")))]);

    assert_eq!(count(&log, "parser", 0), 1);
    assert_eq!(count(&log, "parser", 1), 0);
    assert_eq!(rows_drawing(&log, &Model::default(), 2), [0]);
}

/// A call's own prose is counted as the row it's in draws it: from the kit's blocks in a row of its own, and by the
/// kit's text view in a call's row, as a call in a subagent's work is, while what its call draws after it, such as the
/// subagent's report, is its call's row's own again.
#[test]
fn a_call_s_prose_is_counted_as_the_row_it_s_in_draws_it() {
    let asked = json!({ "description": "Look", "prompt": "Ask about the parser" });
    let log = log([
        event(1, call("helper", "Agent", asked)),
        event(2, within("helper", calls_to("advisor", &["nested"]))),
        event(3, within("helper", from_the_agent(sent_back("nested", "Test the parser first.")))),
        // In the foreground, it reports in its call's result.
        event(4, sent_back("helper", "The parser is sound.")),
        event(5, calls_to("advisor", &["own"])),
        event(6, from_the_agent(sent_back("own", "The parser is fine."))),
    ]);
    let needle = Needle::new("parser").unwrap();
    let by_the_kit = |ix| -> Vec<(Part, bool)> {
        of_row(&log, &Model::default(), &needle, ix).into_iter().map(|found| (found.id.1, found.kit_view)).collect()
    };

    let in_the_agent_s = [(Part::Asked, false), (Part::Advice, true), (Part::Report, false)];
    assert_eq!(by_the_kit(0), in_the_agent_s, "a subagent's call is in its call's row");
    assert_eq!(by_the_kit(4), [(Part::Advice, false)], "and this one in a row of its own");
}

#[test]
fn what_went_wrong_is_found_but_not_what_the_app_says_of_it() {
    let failed = EventBody::TurnEnded {
        outcome: TurnOutcome::Error,
        result: None,
        errors: vec!["the parser crashed".into(), "and the parser after it".into()],
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
    };
    let stopped = |reason| EventBody::AgentExited {
        reason,
        code: Some(1),
        signal: None,
        stderr_tail: vec!["earlier".into(), "panic in the parser".into()],
    };
    let log =
        log([event(1, failed), event(2, stopped(ExitReason::Unexpected)), event(3, stopped(ExitReason::Requested))]);

    assert_eq!(count(&log, "parser", 0), 1, "the error the line says, not the ones after it");
    assert_eq!(count(&log, "parser", 1), 1, "what the agent last printed");
    assert_eq!(count(&log, "earlier", 1), 0, "and only that");
    assert_eq!(count(&log, "stopped unexpectedly", 1), 0, "not what the app says of it");
    assert_eq!(count(&log, "parser", 2), 0, "nor when it was stopped, which says nothing");
}

#[test]
fn a_match_in_a_command_opens_its_call_and_shows_the_command_whole() {
    let log = log([event(1, call("run", "Bash", json!({ "command": "cargo build\ncargo fmt --all" })))]);

    let found = of_row(&log, &Model::default(), &Needle::new("fmt").unwrap(), 0);

    let [found] = &found[..] else { panic!("{found:?}") };
    assert_eq!(found.id.1, Part::Command);
    let key = &found.id.0;
    assert_eq!(found.opens, [(key.clone(), Fold::Output), (key.clone(), Fold::Command)], "past its first line");
}

#[test]
fn a_question_the_agent_asked_isn_t_found_once_it_s_answered_on_its_line() {
    let input = json!({ "questions": [{ "question": "Which parser?", "header": "Parser" }] });
    let question = EventBody::ApprovalRequested {
        approval_id: "q".into(),
        tool_name: "AskUserQuestion".into(),
        tool_use_id: Some("ask".into()),
        input: input.clone(),
        description: None,
        suggestions: serde_json::Value::Null,
        asking: None,
    };
    let answered = EventBody::ApprovalResolved {
        approval_id: "q".into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: Some([("Which parser?".to_string(), "The new parser".to_string())].into()),
        answers_by_position: None,
    };
    let log = log([event(1, call("ask", "AskUserQuestion", input)), event(2, question), event(3, answered)]);

    assert_eq!(count(&log, "parser", 0), 0, "its call");
    assert_eq!(count(&log, "parser", 1), 0, "its line");
}

#[test]
fn images_one_after_another_are_counted_apart_as_they_re_drawn() {
    let log = log([event(1, says("![Before](before.png)\n![After](after.png)"))]);

    // The line break between them would be drawn as a space, were they drawn together.
    assert_eq!(count(&log, " ", 0), 0);
}

#[test]
fn a_compaction_isn_t_found() {
    let outcome = protocol::event::CompactionOutcome::Compacted {
        by: protocol::event::CompactedBy::default(),
        before_tokens: None,
        after_tokens: None,
        took_ms: None,
        summary: Some("The parser was fixed".into()),
    };
    let compacted = EventBody::AgentCompacted { asked: None, keeping: Some("the parser".into()), outcome };
    let log = log([event(1, compacted)]);

    assert_eq!(count(&log, "parser", 0), 0, "what it kept or the summary it opens to");
}
