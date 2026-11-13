use client_core::session_log::{Position, SessionLog};
use protocol::event::{ApprovalOutcome, CallKind, ContentBlock, EventBody, ExitReason, TurnOutcome};
use serde_json::json;

use protocol::find::Needle;
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::Seq;
use protocol::prompt::Prompt;

use super::{count, of_row, rows_drawing};
use crate::find::Part;
use crate::fixtures::{calls_to, event, from_the_agent, log, within};
use crate::folds::Fold;
use crate::transcript::Model;

fn prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text) }
}

fn agent(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

fn says(text: &str) -> EventBody {
    agent(vec![ContentBlock::Text { text: text.into(), images: Vec::new() }])
}

fn call(id: &str, name: &str, input: serde_json::Value) -> EventBody {
    agent(vec![protocol::fixtures::tool_use(id, name, input)])
}

/// `block` as what a call sent back to the agent.
fn came_back(block: ContentBlock) -> EventBody {
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
}

fn sent_back(id: &str, text: &str) -> EventBody {
    came_back(protocol::fixtures::tool_result(id, text, false))
}

/// A call to the tool `name`, recorded with the kind its adapter said it is.
fn call_of(id: &str, name: &str, kind: CallKind, input: serde_json::Value) -> EventBody {
    agent(vec![protocol::fixtures::of_kind(protocol::fixtures::tool_use(id, name, input), kind)])
}

/// What's found of `needle` in the row `ix`, by the part each is in.
fn parts(log: &SessionLog, needle: &str, ix: usize) -> Vec<Part> {
    let needle = Needle::new(needle).unwrap();
    of_row(log, &Model::default(), &needle, ix).into_iter().map(|found| found.id.1).collect()
}

/// The tools here have names no agent's adapter has, so only the kind each call says can be what draws it.
#[test]
fn a_call_is_drawn_as_the_kind_its_adapter_said_whatever_its_tool_is_called() {
    let input = json!({ "command": "cargo test parser" });
    let unsaid = log([event(Seq::new(1), call("c", "shell", input.clone()))]);
    let said = log([event(Seq::new(1), call_of("c", "shell", CallKind::Command, input))]);

    assert_eq!(parts(&unsaid, "parser", 0), [Part::Input], "any tool, by its input");
    assert_eq!(parts(&said, "parser", 0), [Part::Command]);
}

#[test]
fn a_call_its_adapter_said_hands_work_off_gathers_its_subagent_s_work() {
    let work = || event(Seq::new(2), within("helper", says("Looking at the parser")));
    let unsaid = log([event(Seq::new(1), call("helper", "delegate", json!({}))), work()]);
    let said = log([event(Seq::new(1), call_of("helper", "delegate", CallKind::Subagent, json!({}))), work()]);

    assert_eq!((count(&unsaid, "parser", 0), count(&unsaid, "parser", 1)), (0, 1), "in a row of its own");
    assert_eq!((count(&said, "parser", 0), count(&said, "parser", 1)), (1, 0), "in the call's row");
    assert_eq!(rows_drawing(&said, &Model::default(), Position::of_row(1)), [0]);
}

#[test]
fn a_call_its_adapter_said_asks_the_person_something_is_left_to_its_approval() {
    let input = json!({ "about": "Which parser?" });
    let unsaid = log([event(Seq::new(1), call("ask", "inquire", input.clone()))]);
    let said = log([event(Seq::new(1), call_of("ask", "inquire", CallKind::Questions, input))]);

    assert_eq!(count(&unsaid, "parser", 0), 1, "any tool, by its input");
    assert_eq!(count(&said, "parser", 0), 0);
}

/// A failed result that says what kind of call it answers holds what came back alone, so a line in it that reads like
/// how Claude said a command exited is something the command printed, and is found.
#[test]
fn what_a_failed_command_printed_is_found_as_it_came_when_its_result_says_its_kind() {
    let printed = "Exit code 9\nthe parser broke";
    let before = came_back(protocol::fixtures::tool_result("c", printed, true));
    let since = came_back(protocol::fixtures::command_failed("c", printed, 1));
    let made = || event(Seq::new(1), call_of("c", "shell", CallKind::Command, json!({ "command": "make" })));
    let (before, since) = (log([made(), event(Seq::new(2), before)]), log([made(), event(Seq::new(2), since)]));

    assert_eq!(count(&before, "Exit code 9", 0), 0, "from before adapters said, it's how the command exited");
    assert_eq!(count(&since, "Exit code 9", 0), 1);
    assert_eq!((count(&before, "parser", 0), count(&since, "parser", 0)), (1, 1));
}

/// What went wrong is shown the same way under any call: without what its agent put around it to say it failed.
#[test]
fn what_went_wrong_for_any_tool_is_found_without_the_tags_claude_put_around_it() {
    let refused = protocol::fixtures::tool_result("c", "<tool_use_error>no such parser</tool_use_error>", true);
    let log = log([event(Seq::new(1), call("c", "inquire", json!({}))), event(Seq::new(2), came_back(refused))]);

    assert_eq!(count(&log, "tool_use_error", 0), 0);
    assert_eq!(count(&log, "parser", 0), 1);
}

#[test]
fn what_the_person_and_the_agent_wrote_is_found_whatever_its_case() {
    let log = log([
        event(Seq::new(1), prompt("Fix the Parser")),
        event(Seq::new(2), EventBody::PromptTaken { prompt: Seq::new(1) }),
        event(Seq::new(3), says("The **parser** is fixed, and PARSER tests pass.")),
        event(Seq::new(4), agent(vec![ContentBlock::Thinking { text: "The parser was slow".into() }])),
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
        event(Seq::new(1), prompt("Start")),
        event(Seq::new(2), EventBody::PromptTaken { prompt: Seq::new(1) }),
        event(Seq::new(3), prompt("Only the sidebar")),
        event(Seq::new(4), agent(vec![ContentBlock::Thinking { text: "The sidebar first".into() }])),
    ]);
    let needle = |log: &SessionLog| Vec::from_fn(5, |row| count(log, "sidebar", row));

    assert_eq!(needle(&log), [0, 0, 0, 1, 0], "the thought, and not the prompt waiting at the end");
    assert_eq!(rows_drawing(&log, &Model::default(), Position::of_row(2)), Vec::<usize>::new());
    log.apply_event(event(Seq::new(5), EventBody::PromptTaken { prompt: Seq::new(3) }));
    assert_eq!(needle(&log), [0, 0, 0, 1, 1], "under the thought, in the row that took it");
    assert_eq!(rows_drawing(&log, &Model::default(), Position::of_row(2)), [4]);
}

#[test]
fn a_call_s_result_is_found_in_the_call_s_row_which_draws_it() {
    let log = log([
        event(Seq::new(1), call("run", "Bash", json!({ "command": "cargo test", "description": "Run the tests" }))),
        event(Seq::new(2), sent_back("run", "test parser ... FAILED\ntest lexer ... ok")),
    ]);

    assert_eq!(count(&log, "failed", 0), 1, "what it printed");
    assert_eq!(count(&log, "cargo", 0), 1, "the command");
    assert_eq!(count(&log, "the tests", 0), 1, "what the agent said it's for");
    assert_eq!(count(&log, "failed", 1), 0, "the result draws nothing of its own");
    assert_eq!(
        rows_drawing(&log, &Model::default(), Position::of_row(1)),
        [0, 1],
        "but its call's row is counted again"
    );
}

/// A call the person denied never ran, so what the agent was told is found once, as what the person said, and not as
/// what it printed.
#[test]
fn a_denied_calls_words_are_found_as_what_the_person_said() {
    let denied = protocol::fixtures::tool_denied("run", "Use the full path", Some("Use the full path"));
    let log = log([
        event(Seq::new(1), call("run", "Bash", json!({ "command": "rm -rf build" }))),
        event(Seq::new(2), came_back(denied)),
    ]);

    assert_eq!(
        parts(&log, "full path", 0),
        [Part::Said],
        "what the person said, under the call, and not as its output"
    );
    assert_eq!(parts(&log, "rm -rf", 0), [Part::Command], "the command");
}

#[test]
fn what_the_app_writes_itself_isn_t_found() {
    let log = log([
        event(Seq::new(1), call("read", "Read", json!({ "file_path": "/src/main.rs" }))),
        event(Seq::new(2), sent_back("read", "     1\tfn main() {}\n     2\t")),
    ]);

    assert_eq!(count(&log, "main", 0), 2, "the path over the lines, and the line, once it's open");
    assert_eq!(count(&log, "Read", 0), 0, "the line's label");
    assert_eq!(count(&log, "2 lines", 0), 0, "how much it read");
    assert_eq!(count(&log, "1", 0), 0, "the lines' numbers");
}

#[test]
fn a_subagent_s_work_is_found_in_the_call_that_started_it() {
    let log = log([
        event(Seq::new(1), calls_to("Agent", &["helper"])),
        event(Seq::new(2), within("helper", says("Looking at the parser"))),
    ]);

    assert_eq!(count(&log, "parser", 0), 1);
    assert_eq!(count(&log, "parser", 1), 0);
    assert_eq!(rows_drawing(&log, &Model::default(), Position::of_row(1)), [0]);
}

/// A call's own prose is found in the row that draws it, in the order that row shows it: a call in a subagent's work
/// in the row of the call that started the subagent, between what it was asked and its report.
#[test]
fn a_call_s_prose_is_found_in_the_row_that_draws_it_in_the_order_it_shows() {
    let asked = json!({ "description": "Look", "prompt": "Ask about the parser" });
    let log = log([
        event(Seq::new(1), call("helper", "Agent", asked)),
        event(Seq::new(2), within("helper", calls_to("advisor", &["nested"]))),
        event(Seq::new(3), within("helper", from_the_agent(sent_back("nested", "Test the parser first.")))),
        // In the foreground, it reports in its call's result.
        event(Seq::new(4), sent_back("helper", "The parser is sound.")),
        event(Seq::new(5), calls_to("advisor", &["own"])),
        event(Seq::new(6), from_the_agent(sent_back("own", "The parser is fine."))),
    ]);
    let needle = Needle::new("parser").unwrap();
    let parts = |ix| -> Vec<Part> {
        of_row(&log, &Model::default(), &needle, ix).into_iter().map(|found| found.id.1).collect()
    };

    assert_eq!(parts(0), [Part::Asked, Part::Advice, Part::Report], "a subagent's call is in its call's row");
    assert_eq!(parts(4), [Part::Advice], "and this one in a row of its own");
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
        request_failed: false,
    };
    let stopped = |reason| EventBody::AgentExited {
        reason,
        code: Some(1),
        signal: None,
        stderr_tail: vec!["earlier".into(), "panic in the parser".into()],
    };
    let log = log([
        event(Seq::new(1), failed),
        event(Seq::new(2), stopped(ExitReason::Unexpected)),
        event(Seq::new(3), stopped(ExitReason::Requested)),
    ]);

    assert_eq!(count(&log, "parser", 0), 1, "the error the line says, not the ones after it");
    assert_eq!(count(&log, "parser", 1), 1, "what the agent last printed");
    assert_eq!(count(&log, "earlier", 1), 0, "and only that");
    assert_eq!(count(&log, "stopped unexpectedly", 1), 0, "not what the app says of it");
    assert_eq!(count(&log, "parser", 2), 0, "nor when it was stopped, which says nothing");
}

#[test]
fn a_match_in_a_command_opens_its_call() {
    let log = log([event(Seq::new(1), call("run", "Bash", json!({ "command": "cargo build\ncargo fmt --all" })))]);

    let found = of_row(&log, &Model::default(), &Needle::new("fmt").unwrap(), 0);

    let [found] = &found[..] else { panic!("{found:?}") };
    assert_eq!(found.id.1, Part::Command);
    let key = &found.id.0;
    assert_eq!(found.opens, [(key.clone(), Fold::Output)], "which shows the command whole");
}

#[test]
fn a_question_the_agent_asked_isn_t_found_once_it_s_answered_on_its_line() {
    let input = json!({ "questions": [{ "question": "Which parser?", "header": "Parser" }] });
    let question: EventBody = ApprovalRequest {
        tool_use_id: Some("ask".into()),
        input: input.clone(),
        ..approval_request("q", "AskUserQuestion")
    }
    .into();
    let answered = EventBody::ApprovalResolved {
        approval_id: "q".into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: Some([("Which parser?".to_string(), "The new parser".to_string())].into()),
        answers_by_position: None,
        by_setting: false,
    };
    let log = log([
        event(Seq::new(1), call("ask", "AskUserQuestion", input)),
        event(Seq::new(2), question),
        event(Seq::new(3), answered),
    ]);

    assert_eq!(count(&log, "parser", 0), 0, "its call");
    assert_eq!(count(&log, "parser", 1), 0, "its line");
}

#[test]
fn images_one_after_another_are_counted_apart_as_they_re_drawn() {
    let log = log([event(Seq::new(1), says("![Before](before.png)\n![After](after.png)"))]);

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
    let log = log([event(Seq::new(1), compacted)]);

    assert_eq!(count(&log, "parser", 0), 0, "what it kept or the summary it opens to");
}
