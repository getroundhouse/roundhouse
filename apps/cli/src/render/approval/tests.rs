use client_core::tool_servers::ToolServerNames;
use client_core::{Answered, Approval};
use protocol::capability::approvals::{Asks, Decision};
use protocol::capability::asking::{Question, QuestionAnswer, QuestionOption};
use protocol::capability::calls::Call;
use protocol::capability::calls::call::{Command, Edit, FilePatch, Patch, PatchChange, PatchHunk, Replace, Replaces};
use protocol::event::{HunkLine, LineMark, SessionEvent};
use protocol::fixtures::{ApprovalRequest, allow_once, approval_request, deny, question, typed_command, typed_request};
use protocol::model::{Records, Seq, ToolServer, ToolServerName};
use serde_json::json;

use super::{requested, resolved};

fn event(asks: Asks) -> SessionEvent {
    protocol::fixtures::event(Seq::new(7), approval_request("req_1", asks).into())
}

/// A request to make `call`.
fn asked_to(call: Call) -> ApprovalRequest {
    approval_request("req_1", Asks::Call { call })
}

fn lines(event: &SessionEvent) -> Vec<String> {
    lines_naming(event, &ToolServerNames::default())
}

fn lines_naming(event: &SessionEvent, servers: &ToolServerNames) -> Vec<String> {
    requested(event, &Approval::of(&event.body).unwrap(), servers)
}

fn colors() -> Question {
    let mut colors = Question { header: Some("Color".into()), ..question("Red or blue?", &["Red", "Blue"], true) };
    colors.options[0] = QuestionOption { description: Some("Warm".into()), ..colors.options[0].clone() };
    colors
}

#[test]
fn a_question_prints_its_options_numbered_and_how_to_answer() {
    let printed = lines(&event(Asks::Questions { questions: vec![colors()] }));

    assert_eq!(
        printed,
        [
            "QUESTION: Color: Red or blue?",
            "  1. Red: Warm",
            "  2. Blue",
            "  (any number of these)",
            r#"  roundhouse answer s req_1 "Red or blue?" <answer>"#,
        ]
    );
}

#[test]
fn a_plan_prints_whole_and_a_tool_by_its_input() {
    let planned = lines(&event(protocol::fixtures::plan("# Plan\n\n1. Read")));
    let tool = lines(&protocol::fixtures::event(Seq::new(7), asked_to(ls()).into()));

    assert_eq!(
        planned,
        ["PLAN:", "  # Plan", "", "  1. Read", "  roundhouse approve s req_1, or with --deny <what to change>"]
    );
    assert_eq!(tool, ["APPROVAL NEEDED: Command: ls -la", "  roundhouse approve s req_1"]);
}

fn ls() -> Call {
    Call::Command(typed_command("ls -la"))
}

#[test]
fn a_tools_use_prints_its_command_whole_under_what_its_for_with_why_and_who_asks() {
    let command = Command {
        description: Some("Remove the scratch directory".into()),
        ..typed_command("D=\"$(cat path)\"\nrm -rf \"$D\"")
    };
    let mut asked = asked_to(Call::Command(command));
    asked.request.why = Some("The target is only known once it runs".into());
    asked.asked_by = Some("toolu_agent".into());

    let printed = lines(&protocol::fixtures::event(Seq::new(7), asked.into()));

    assert_eq!(
        printed,
        [
            "APPROVAL NEEDED: Command: Remove the scratch directory",
            r#"  $ D="$(cat path)""#,
            r#"  $ rm -rf "$D""#,
            "  why: The target is only known once it runs",
            "  asked by the subagent toolu_agent started",
            "  roundhouse approve s req_1",
        ]
    );
}

#[test]
fn an_edit_prints_its_file_and_what_it_takes_out_and_puts_in() {
    let replace = Replace { old: "let a = 1;".into(), new: "let a = 2;\nlet b = 3;".into(), replaces: Replaces::One };
    let edit = Call::Edit(Edit { path: "src/lib.rs".into(), edits: vec![replace] });

    let plain = lines(&protocol::fixtures::event(Seq::new(7), asked_to(edit).into()));

    let change = ["  - let a = 1;", "  + let a = 2;", "  + let b = 3;", "  roundhouse approve s req_1"];
    assert_eq!(plain[0], "APPROVAL NEEDED: Edit: src/lib.rs", "its file said once");
    assert_eq!(plain[1..], change);
}

#[test]
fn a_tool_with_no_kind_of_its_own_prints_its_server_as_the_person_calls_it_and_its_whole_input() {
    let names = {
        let name = ToolServerName::stored("GitHub (Northwind)".into());
        let northwind = ToolServer { name, ..protocol::fixtures::tool_server("t") };
        crate::run::held(Records { tool_servers: vec![northwind], ..Records::default() }).tool_server_names().clone()
    };
    let printed = |call| lines_naming(&protocol::fixtures::event(Seq::new(7), asked_to(call).into()), &names);
    let mcp = |server: &str, input| Call::Mcp { server: server.into(), tool: "create_issue".into(), input };

    let filing = printed(mcp("GitHub_Northwind", json!({ "labels": ["kind/feature"] })));

    assert_eq!(
        filing,
        [
            "APPROVAL NEEDED: Create issue · GitHub (Northwind)",
            "  {",
            "    \"labels\": [",
            "      \"kind/feature\"",
            "    ]",
            "  }",
            "  roundhouse approve s req_1",
        ]
    );
    let approve = "  roundhouse approve s req_1";
    assert_eq!(printed(mcp("sentry", json!({}))), ["APPROVAL NEEDED: Create issue · sentry", approve], "the CLI's own");
    let of_none = printed(Call::Other { tool: "seek".into(), input: json!({}) });
    assert_eq!(of_none, ["APPROVAL NEEDED: Seek", approve]);
}

/// A patch prints each file it changes and what it does to each, as an edit's lines are printed, and a patch of one
/// file has it on its first line.
#[test]
fn a_patch_prints_each_file_and_what_it_does_to_it() {
    let line = |mark, text: &str| HunkLine { mark, text: text.into() };
    let lines_of = |files| {
        lines(&protocol::fixtures::event(Seq::new(7), typed_request("req_1", "p", Call::Patch(Patch { files }))))
    };
    let hunk = PatchHunk {
        after: None,
        lines: vec![line(LineMark::Same, "fn a() {"), line(LineMark::Removed, "1"), line(LineMark::Added, "2")],
    };
    let changed =
        FilePatch { path: "src/lib.rs".into(), change: PatchChange::Update { move_to: None, hunks: vec![hunk] } };
    let deleted = FilePatch { path: "old.rs".into(), change: PatchChange::Delete };

    let both = lines_of(vec![changed.clone(), deleted]);
    let one = lines_of(vec![changed]);

    let change = ["    fn a() {", "  - 1", "  + 2"];
    assert_eq!(both[..2], ["APPROVAL NEEDED: Edit", "  file: src/lib.rs"]);
    assert_eq!(both[2..5], change);
    assert_eq!(both[5..], ["  file: old.rs", "  deleted", "  roundhouse approve s req_1"]);
    assert_eq!(one[0], "APPROVAL NEEDED: Edit: src/lib.rs", "its file said once");
    assert_eq!(one[1..4], change);
}

#[test]
fn what_was_answered_prints_by_question() {
    let why = question("Why?", &[], false);
    let answers = vec![QuestionAnswer { picked: vec![0, 1], typed: None }, QuestionAnswer::default()];
    let answer = Decision::Answer { answers: answers.clone() };

    let printed = resolved("req_1", Some(&answer), Some(&Answered(answers)), &[colors(), why]);

    assert_eq!(printed, ["approval req_1 Answered", "  Red or blue?: Red, Blue", "  Why?: not answered"]);
    assert_eq!(resolved("req_1", Some(&deny(None)), None, &[colors()]), ["approval req_1 Denied"]);
    assert_eq!(resolved("req_1", Some(&allow_once()), None, &[]), ["approval req_1 Allowed"]);
    assert_eq!(resolved("req_1", None, None, &[]), ["approval req_1 Cancelled"], "nobody decided");
}
