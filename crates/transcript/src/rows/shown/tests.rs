use client_core::session_log::{Position, SessionLog};
use protocol::capability::approvals::Decision;
use protocol::capability::asking::QuestionAnswer;
use protocol::event::{ContentBlock, EventBody, ExitReason, TurnOutcome};
use serde_json::{Value, json};

use protocol::capability::calls::call::{Command, MessageSubagent, Read, StartSubagent};
use protocol::capability::calls::output::{Ended, Lines};
use protocol::capability::calls::{Call, CallOutput};
use protocol::find::Needle;
use protocol::fixtures::{
    approval_request, asking, command_output, decided, question, typed_call, typed_command, typed_denied, typed_result,
    typed_subagent,
};
use protocol::model::Seq;
use protocol::prompt::Prompt;

use super::{count, of_row, rows_drawing};
use crate::find::Part;
use crate::fixtures::{advised, event, from_the_agent, log, subagents, within};
use crate::folds::Fold;
use crate::transcript::Model;

fn prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text) }
}

fn agent(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

fn says(text: &str) -> EventBody {
    agent(vec![ContentBlock::Text { text: text.into(), images: Vec::new() }])
}

/// The agent making `call`, as the call `id`.
fn call(id: &str, call: Call) -> EventBody {
    agent(vec![typed_call(id, call)])
}

/// Running `command`, which the agent says is for `described`, if anything.
fn command(command: &str, described: Option<&str>) -> Call {
    Call::Command(Command { description: described.map(Into::into), ..typed_command(command) })
}

/// A call to a tool of no kind a client knows, by the agent's name for it, and what it was given.
fn other(tool: &str, input: Value) -> Call {
    Call::Other { tool: tool.into(), input }
}

/// `block` as what a call sent back to the agent.
fn came_back(block: ContentBlock) -> EventBody {
    EventBody::AgentUser { subagent: None, blocks: vec![block] }
}

/// What came back for the call `id`, as words.
fn sent_back(id: &str, text: &str) -> EventBody {
    came_back(typed_result(id, CallOutput::Words { text: text.into() }, false))
}

/// What the command made as the call `id` printed, and that it exited with `code`, which failed unless it's 0.
fn printed(id: &str, text: &str, code: i32) -> EventBody {
    came_back(typed_result(id, command_output(text, Some(code), Ended::Exited), code != 0))
}

/// What's found of `needle` in the row `ix`, by the part each is in.
fn parts(log: &SessionLog, needle: &str, ix: usize) -> Vec<Part> {
    let needle = Needle::new(needle).unwrap();
    of_row(log, &Model::default(), &needle, ix).into_iter().map(|found| found.id.1).collect()
}

/// Only the kind of each call can be what draws it: a command by its command, and a tool of no kind a client knows by
/// what it was given.
#[test]
fn a_call_is_drawn_as_its_kind_and_one_of_no_kind_a_client_knows_by_its_input() {
    let unknown = log([event(Seq::new(1), call("c", other("shell", json!({ "command": "cargo test parser" }))))]);
    let typed = log([event(Seq::new(1), call("c", command("cargo test parser", None)))]);

    assert_eq!(parts(&unknown, "parser", 0), [Part::Input], "any tool, by its input");
    assert_eq!(parts(&typed, "parser", 0), [Part::Command]);
}

#[test]
fn a_call_that_hands_work_off_gathers_its_subagent_s_work() {
    let work = || event(Seq::new(2), within("helper", says("Looking at the parser")));
    let unknown = log([event(Seq::new(1), call("helper", other("delegate", json!({})))), work()]);
    let typed = log([event(Seq::new(1), call("helper", Call::Subagent(typed_subagent("Look.")))), work()]);

    assert_eq!((count(&unknown, "parser", 0), count(&unknown, "parser", 1)), (0, 1), "in a row of its own");
    assert_eq!((count(&typed, "parser", 0), count(&typed, "parser", 1)), (1, 0), "in the call's row");
    assert_eq!(rows_drawing(&typed, &Model::default(), Position::of_row(1)), [0]);
}

/// A failed command's result holds what it printed alone, so a line in it that reads like how Claude said a command
/// exited is something the command printed, and is found.
#[test]
fn what_a_failed_command_printed_is_found_as_it_came() {
    let log = log([
        event(Seq::new(1), call("c", command("make", None))),
        event(Seq::new(2), printed("c", "Exit code 9\nthe parser broke", 1)),
    ]);

    assert_eq!(count(&log, "Exit code 9", 0), 1);
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
        event(Seq::new(1), call("run", command("cargo test", Some("Run the tests")))),
        event(Seq::new(2), printed("run", "test parser ... FAILED\ntest lexer ... ok", 0)),
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

/// A message to a subagent at work is found as its call shows it open: what it said, as what the subagent was asked,
/// then what came back. The subagent's ID, which only names it, isn't.
#[test]
fn a_message_to_a_subagent_is_found_as_what_it_was_asked() {
    let message = MessageSubagent { subagent: "thread-1".into(), message: "Look at the parser too".into() };
    let came = CallOutput::Words { text: "thread-1 is on the parser".into() };
    let log = log([
        event(Seq::new(1), agent(vec![typed_call("tell", Call::MessageSubagent(message))])),
        event(Seq::new(2), came_back(typed_result("tell", came, false))),
    ]);

    assert_eq!(parts(&log, "parser", 0), [Part::Asked, Part::Output]);
    assert_eq!(parts(&log, "thread-1", 0), [Part::Output], "only where it came back");
}

/// A call the person denied never ran, so what the agent was told is found once, as what the person said, and not as
/// what it printed.
#[test]
fn a_denied_calls_words_are_found_as_what_the_person_said() {
    let denied = typed_denied("run", "Use the full path", Some("Use the full path"));
    let log =
        log([event(Seq::new(1), call("run", command("rm -rf build", None))), event(Seq::new(2), came_back(denied))]);

    assert_eq!(
        parts(&log, "full path", 0),
        [Part::Said],
        "what the person said, under the call, and not as its output"
    );
    assert_eq!(parts(&log, "rm -rf", 0), [Part::Command], "the command");
}

/// Failures in a row are one card, whose words are found once, in the newest, which draws them.
#[test]
fn a_failure_said_in_a_later_card_isn_t_found_again() {
    let first = protocol::fixtures::request_failed("You've reached your limit.", None);
    let mut second = first.clone();
    if let EventBody::RequestFailed { again, .. } = &mut second {
        *again = Some(protocol::event::Again { previous: Seq::new(1), times: 2 });
    }
    let log = log([event(Seq::new(1), first), event(Seq::new(2), second)]);

    assert_eq!(parts(&log, "limit", 0), [], "the card after says it");
    assert_eq!(parts(&log, "limit", 1), [Part::FailedRequest]);
}

#[test]
fn what_the_app_writes_itself_isn_t_found() {
    let lines = Lines { first: 1, lines: vec!["fn main() {}".into(), String::new()], total: None };
    let log = log([
        event(Seq::new(1), call("read", Call::Read(Read { path: "/src/main.rs".into(), lines: None, pages: None }))),
        event(Seq::new(2), came_back(typed_result("read", CallOutput::Lines(lines), false))),
    ]);

    assert_eq!(count(&log, "main", 0), 2, "the path over the lines, and the line, once it's open");
    assert_eq!(count(&log, "Read", 0), 0, "the line's label");
    assert_eq!(count(&log, "2 lines", 0), 0, "how much it read");
    assert_eq!(count(&log, "1", 0), 0, "the lines' numbers");
}

#[test]
fn a_subagent_s_work_is_found_in_the_call_that_started_it() {
    let log = log([
        event(Seq::new(1), subagents(&["helper"])),
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
    let asked = StartSubagent { description: Some("Look".into()), ..typed_subagent("Ask about the parser") };
    let report = CallOutput::Report { report: "The parser is sound.".into() };
    let log = log([
        event(Seq::new(1), call("helper", Call::Subagent(asked))),
        event(Seq::new(2), within("helper", advised(&["nested"]))),
        event(Seq::new(3), within("helper", from_the_agent(sent_back("nested", "Test the parser first.")))),
        // In the foreground, it reports in its call's result.
        event(Seq::new(4), came_back(typed_result("helper", report, false))),
        event(Seq::new(5), advised(&["own"])),
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
    let log = log([event(Seq::new(1), call("run", command("cargo build\ncargo fmt --all", None)))]);

    let found = of_row(&log, &Model::default(), &Needle::new("fmt").unwrap(), 0);

    let [found] = &found[..] else { panic!("{found:?}") };
    assert_eq!(found.id.1, Part::Command);
    let key = &found.id.0;
    assert_eq!(found.opens, [(key.clone(), Fold::Output)], "which shows the command whole");
}

#[test]
fn a_question_the_agent_asked_isn_t_found_once_it_s_answered_on_its_line() {
    let mut request = approval_request("q", asking(vec![question("Which parser?", &[], false)]));
    request.tool_use_id = Some("ask".into());
    let question: EventBody = request.into();
    let typed = QuestionAnswer { picked: Vec::new(), typed: Some("The new parser".into()) };
    let answered = decided("q", Decision::Answer { answers: vec![typed] });
    let log = log([
        event(Seq::new(1), call("ask", Call::Questions)),
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
