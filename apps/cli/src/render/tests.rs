use client_core::calls::{Recorded, Returned};
use protocol::capability::calls::call::{Read, Search, Shows};
use protocol::capability::calls::output::Ended;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{
    Again, AgentTask, ContentBlock, Denied, EndedTask, FailureKind, RequestFailure, TaskKind, TaskOutcome,
};
use protocol::fixtures;
use protocol::model::{EndedWait, Seq, WaitEnd, WaitEnded};
use serde_json::json;

use super::{Printer, called, came_back, denial, own_turn, request_failed, started};

fn task(id: &str, kind: TaskKind) -> AgentTask {
    AgentTask { id: id.into(), description: Some(format!("Do {id}")), kind, call: None, subagent: None }
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

/// So the terminal reads a call as the app does: by its kind, and one of no kind a client knows by its tool's name.
#[test]
fn a_call_says_what_it_s_about_as_every_client_does() {
    let said = |call: Call| called(Recorded::of(&fixtures::typed_call("c", call)).unwrap());
    let search = Search { pattern: "fn main".into(), path: Some("src".into()), files: None, shows: Shows::Lines };
    let read = Read { path: "src/a.rs".into(), lines: None, pages: None };
    let other = Call::Other { tool: "seek".into(), input: json!({ "path": "src", "pattern": "fn main" }) };

    assert_eq!(said(Call::Command(fixtures::typed_command("\ncargo test\n--workspace"))), "Command: cargo test");
    assert_eq!(said(Call::Read(read)), "Read: src/a.rs");
    assert_eq!(said(Call::Search(search)), "Search: fn main", "by what it looks for");
    assert_eq!(said(other), "seek: src", "any tool, by the first of its usual inputs");
    assert_eq!(said(Call::Advisor), "Ask the advisor", "it takes no input");
}

/// What's printed of the result `block`.
fn printed(block: &ContentBlock) -> Vec<String> {
    came_back(&Returned::of(block).expect("a result"))
}

#[test]
fn a_failed_result_says_how_the_command_exited() {
    let words = |text: &str| CallOutput::Words { text: text.into() };
    let failed =
        fixtures::typed_result("c", fixtures::command_output("ls: x: No such file", Some(2), Ended::Exited), true);

    assert_eq!(printed(&failed), ["tool error, exit 2: ls: x: No such file"]);
    assert_eq!(printed(&fixtures::typed_result("c", words("no such tool"), true)), ["tool error: no such tool"]);
    assert_eq!(printed(&fixtures::typed_result("c", words("a.rs"), false)), ["tool result: a.rs"]);
    let timed_out =
        fixtures::typed_result("c", fixtures::command_output("still building", None, Ended::TimedOut), true);
    assert_eq!(printed(&timed_out), ["tool error, timed out: still building"]);
}

/// A typed result prints as one recorded as the agent's words does: its first lines, how many images came back, and
/// that it was denied or is for the model alone.
#[test]
fn a_typed_result_prints_as_much_as_any_other() {
    let long = (1..=9).map(|line| format!("line {line}")).collect::<Vec<_>>().join("\n");
    let words = |text: &str| CallOutput::Words { text: text.into() };
    let with_an_image =
        fixtures::with_images(fixtures::typed_result("c", words("a screenshot"), false), vec![fixtures::image("i")]);
    let backgrounded = Ended::Backgrounded { task: "t".into() };
    let in_the_background =
        fixtures::typed_result("c", fixtures::command_output("Compiling", None, backgrounded), false);

    let first = printed(&fixtures::typed_result("c", words(&long), false));
    assert_eq!(first[..2], ["tool result: line 1", "  line 2"]);
    assert_eq!(first.last().map(String::as_str), Some("  ... 3 more lines"));
    assert_eq!(printed(&with_an_image), ["tool result: a screenshot [1 image]"]);
    assert_eq!(printed(&fixtures::typed_denied("c", "The user said no", Some("Not that"))), ["tool denied: Not that"]);
    let encrypted = fixtures::typed_result("c", CallOutput::Encrypted, false);
    assert_eq!(printed(&encrypted), ["tool result: encrypted, for the model alone"]);
    assert_eq!(printed(&in_the_background), ["tool result, went on in the background: Compiling"]);
    let any_other = fixtures::typed_result("c", CallOutput::Backgrounded { task: Some("t".into()) }, false);
    assert_eq!(printed(&any_other), ["tool result, went on in the background"], "nothing came back but that");
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

    assert_eq!(printer.ending(&"toolu_a".into(), TaskOutcome::Finished), "a subagent finished: Find the tests");
    assert_eq!(printer.ending(&"toolu_b".into(), TaskOutcome::Stopped), "a subagent stopped: toolu_b");
    assert_eq!(
        printer.ending(&"toolu_c".into(), TaskOutcome::Failed),
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

#[test]
fn a_turn_the_agent_started_by_itself_says_which_tasks_it_heard_had_ended() {
    let ended = |id: &str, description: Option<&str>, outcome| EndedTask {
        task: AgentTask {
            id: id.into(),
            description: description.map(String::from),
            kind: TaskKind::Command,
            call: None,
            subagent: None,
        },
        outcome,
        wait: None,
    };

    let said =
        own_turn(&[ended("b1", Some("Run the tests"), TaskOutcome::Failed), ended("b2", None, TaskOutcome::Finished)]);

    assert_eq!(
        said, "the agent picked up by itself, after: Run the tests failed; b2 finished",
        "by its ID with no name"
    );
    let mut wait = ended("k3", Some("roundhouse/wait_for_pull_requests"), TaskOutcome::Failed);
    wait.task.kind = TaskKind::Wait;
    let unrecorded = "the agent picked up by itself, after: waited on a pull request (failed)";
    assert_eq!(own_turn(std::slice::from_ref(&wait)), unrecorded, "not by its tool's name");
    let mut kept = fixtures::pull_request_wait("k3");
    kept.ended = Some(WaitEnded { at_ms: 2, how: WaitEnd::CalledOffByPerson });
    wait.wait = Some(EndedWait::from(&kept));
    let as_the_app_says = "the agent picked up by itself, after: waited on #7: required checks (Stopped)";
    assert_eq!(own_turn(&[wait]), as_the_app_says, "from the hub's record, not the agent's word");
    assert_eq!(own_turn(&[]), "the agent picked up by itself");
}

#[test]
fn a_failed_request_that_repeats_the_one_before_says_how_many_times_in_a_row() {
    let failure =
        RequestFailure { message: "You've hit your limit.".into(), kind: FailureKind::Limit, resets_at_ms: None };

    assert_eq!(request_failed(&failure, None), "request failed: You've hit your limit.");
    let again = Again { previous: Seq::new(12), times: 3 };
    assert_eq!(request_failed(&failure, Some(again)), "request failed again, 3 times in a row: You've hit your limit.");
}
