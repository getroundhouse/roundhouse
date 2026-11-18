use protocol::capability::approvals::Asks;
use protocol::capability::asking::QuestionAnswer;
use protocol::capability::calls::Call;
use protocol::capability::calls::call::{Command, Edit, Fetch, Replace, Replaces, Write};
use protocol::event::EventBody;
use protocol::fixtures::{self, ApprovalRequest, allow_once, approval_request, asking, deny, plan};
use protocol::model::UseChoice;
use serde_json::json;

use super::{Given, ToolUseAnswer, ToolUseAsked};
use crate::approval::Approval;
use crate::calls::{EditFile, WriteFile};

const COMMAND: &str = "rm -rf \"$(cat path)\"\necho done";

/// A request to make `call`.
fn request(call: Call) -> ApprovalRequest {
    approval_request("a", Asks::Call { call })
}

/// Running `command`, which the agent said is for `described`, if it said.
fn command(command: &str, described: Option<&str>) -> Call {
    Call::Command(Command { description: described.map(str::to_string), ..fixtures::typed_command(command) })
}

#[test]
fn a_command_says_what_its_for_itself_whole_why_and_which_subagent_asks() {
    let mut asked = request(command(COMMAND, Some("Remove the scratch directory")));
    asked.request.why = Some("Its target is only known once it runs".into());
    asked.asked_by = Some("toolu_agent".into());
    let body: EventBody = asked.into();

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(asked.tool, "Command", "what the call is, in words, rather than the agent's name for its tool");
    assert_eq!(asked.about, Some("Remove the scratch directory"));
    assert_eq!(asked.given, Some(Given::Command(COMMAND)), "every line of it");
    assert_eq!(asked.reason, Some("Its target is only known once it runs"));
    assert_eq!(asked.asked_by, Some(&"toolu_agent".into()));
}

#[test]
fn a_command_is_for_what_its_agent_said_unless_that_was_only_space() {
    let blank: EventBody = request(command("ls", Some("  "))).into();
    let nothing_said: EventBody = request(command("ls", None)).into();

    assert_eq!(ToolUseAsked::of(&blank).unwrap().about, None);
    let nothing_said = ToolUseAsked::of(&nothing_said).unwrap();
    assert_eq!((nothing_said.about, nothing_said.reason, nothing_said.asked_by), (None, None, None));
}

#[test]
fn an_edit_or_a_write_is_given_what_it_would_change_and_another_tool_its_main_input() {
    let replace = Replace { old: "a".into(), new: "b".into(), replaces: Replaces::One };
    let edit: EventBody = request(Call::Edit(Edit { path: "src/lib.rs".into(), edits: vec![replace] })).into();
    let write: EventBody = request(Call::Write(Write { path: "notes.md".into(), content: "# Notes\n".into() })).into();
    let fetch: EventBody = request(Call::Fetch(Fetch { url: "https://example.com".into(), asking: None })).into();

    let edit = ToolUseAsked::of(&edit).unwrap();
    let write = ToolUseAsked::of(&write).unwrap();

    assert_eq!((edit.tool.as_str(), edit.about), ("Edit", None));
    assert_eq!(edit.given, Some(Given::Edit(EditFile { path: "src/lib.rs", edits: vec![("a", "b")] })));
    assert_eq!(write.given, Some(Given::Write(WriteFile { path: "notes.md", content: "# Notes\n" })));
    assert_eq!((edit.given.unwrap().path(), write.given.unwrap().path()), (Some("src/lib.rs"), Some("notes.md")));
    let fetch = ToolUseAsked::of(&fetch).unwrap();
    assert_eq!(fetch.given, Some(Given::MainInput("https://example.com".into())));
}

#[test]
fn a_tool_with_no_kind_of_its_own_has_a_readable_name_and_its_main_input() {
    let post = Call::Mcp { server: "team".into(), tool: "post_message".into(), input: json!({ "path": "notes.md" }) };
    let body: EventBody = request(post).into();

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(asked.tool, "Post message");
    assert_eq!((asked.about, asked.given), (None, Some(Given::MainInput("notes.md".into()))));
}

#[test]
fn only_a_call_is_a_tools_use() {
    let asks = |asks: Asks| -> EventBody { approval_request("a", asks).into() };

    assert!(ToolUseAsked::of(&asks(Asks::Unknown)).is_none(), "what this build can't read");
    assert!(ToolUseAsked::of(&asks(asking(Vec::new()))).is_none());
    assert!(ToolUseAsked::of(&asks(plan(""))).is_none());
    assert!(ToolUseAsked::of(&EventBody::Unknown).is_none());
}

#[test]
fn an_answer_allows_or_denies_by_its_place_and_whats_typed_denies_in_those_words() {
    let body: EventBody = request(command("ls", None)).into();
    let approval = Approval::of(&body).unwrap();
    let picked = |rows: &[u32]| QuestionAnswer { picked: rows.to_vec(), typed: None };
    let typed = QuestionAnswer { picked: vec![0], typed: Some("Use the full path".into()) };

    assert_eq!(ToolUseAnswer::ALL, [ToolUseAnswer::Allow, ToolUseAnswer::Deny], "the order every client shows");
    assert_eq!(approval.deciding_tool_use(&picked(&[0])), Some(allow_once()));
    assert_eq!(approval.deciding_tool_use(&picked(&[1])), Some(deny(None)), "with the words left to the adapter");
    assert_eq!(approval.deciding_tool_use(&typed), Some(deny(Some("Use the full path"))), "whatever row came with it");
    for neither in [picked(&[]), picked(&[7]), picked(&[0, 1])] {
        assert_eq!(approval.deciding_tool_use(&neither), None, "{neither:?}");
    }
}

#[test]
fn a_draft_keeps_an_answer_by_name() {
    for answer in ToolUseAnswer::ALL {
        assert_eq!(ToolUseAnswer::of(answer.chosen()), Some(answer));
        assert_eq!(ToolUseAnswer::at(answer.place()), Some(answer));
    }
    assert_eq!(ToolUseAnswer::of(UseChoice::Unknown), None, "one this build doesn't know");
}

/// What's asked of a command is whether to run it, so one that only reads a file, which the chat draws as reading it,
/// is asked about as the command it is.
#[test]
fn a_typed_command_that_only_reads_is_asked_about_as_a_command() {
    use protocol::capability::calls::call::CommandPart;
    let reads = vec![CommandPart::Read { path: "secrets.env".into() }];
    let call = Call::Command(Command { parts: reads, ..protocol::fixtures::typed_command("cat secrets.env") });
    let body = protocol::fixtures::typed_request("a", "run-cat", call);

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(asked.given, Some(Given::Command("cat secrets.env")));
}

/// A patch is asked about by the files it changes and how, which is all the person has to go on, and one of one file
/// by that file.
#[test]
fn a_typed_patch_is_asked_about_by_its_changes() {
    use protocol::capability::calls::call::{FilePatch, Patch, PatchChange};
    let add = PatchChange::Add { content: "fn may_pass() {}".into() };
    let patch = Patch { files: vec![FilePatch { path: "src/signal.rs".into(), change: add }] };
    let body = protocol::fixtures::typed_request("a", "patch-1", Call::Patch(patch.clone()));

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(asked.tool, "Edit");
    assert_eq!(asked.given, Some(Given::Patch(&patch)));
    assert_eq!(asked.given.as_ref().and_then(Given::path), Some("src/signal.rs"));
    let mut both = patch.clone();
    both.files.push(FilePatch { path: "src/depot.rs".into(), change: PatchChange::Delete });
    assert_eq!(Given::Patch(&both).path(), None, "of no one file");
}

/// A typed call of any other kind is asked about by what its line says, not by its input as JSON.
#[test]
fn a_typed_call_of_any_other_kind_is_asked_about_as_its_line_says() {
    use protocol::capability::calls::call::{Schedule, When};
    let schedule = Schedule { prompt: "Check #182's CI".into(), when: When::After { seconds: 1200 } };
    let body = protocol::fixtures::typed_request("a", "later-1", Call::Schedule(schedule));

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(
        (asked.tool.as_str(), asked.given),
        ("Schedule a prompt", Some(Given::MainInput("Check #182's CI".into())))
    );
}
