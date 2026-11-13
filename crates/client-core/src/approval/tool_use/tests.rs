use protocol::event::{ApprovalDecision, Asking, CallKind, EventBody, QuestionAnswer};
use protocol::fixtures::{ApprovalRequest, approval_request};
use serde_json::{Value, json};

use super::{Given, ToolUseAnswer, ToolUseAsked};
use crate::approval::Approval;
use crate::calls::{EditFile, WriteFile};

const COMMAND: &str = "rm -rf \"$(cat path)\"\necho done";

fn request(tool: &str, kind: CallKind, input: Value, description: Option<&str>) -> ApprovalRequest {
    ApprovalRequest {
        input,
        description: description.map(str::to_string),
        asking: Some(Asking::ToolUse { call: Some(kind) }),
        ..approval_request("a", tool)
    }
}

#[test]
fn a_command_says_what_its_for_itself_whole_why_and_which_subagent_asks() {
    let body: EventBody = ApprovalRequest {
        reason: Some("Its target is only known once it runs".into()),
        asked_by: Some("toolu_agent".into()),
        ..request("Bash", CallKind::Command, json!({ "command": COMMAND }), Some("Remove the scratch directory"))
    }
    .into();

    let asked = ToolUseAsked::of(&body).unwrap();

    assert_eq!(asked.tool, "Bash");
    assert_eq!(asked.about, Some("Remove the scratch directory"));
    assert_eq!(asked.given, Some(Given::Command(COMMAND)), "every line of it");
    assert_eq!(asked.reason, Some("Its target is only known once it runs"));
    assert_eq!(asked.asked_by, Some(&"toolu_agent".into()));
}

#[test]
fn a_command_is_for_what_its_input_says_when_the_request_says_nothing_or_only_space() {
    let described = json!({ "command": "ls", "description": "List what's here" });
    let said_in_its_input: EventBody = request("Bash", CallKind::Command, described.clone(), None).into();
    let blank: EventBody = request("Bash", CallKind::Command, described, Some("  ")).into();
    let nothing_said: EventBody = request("Bash", CallKind::Command, json!({ "command": "ls" }), None).into();

    assert_eq!(ToolUseAsked::of(&said_in_its_input).unwrap().about, Some("List what's here"));
    assert_eq!(ToolUseAsked::of(&blank).unwrap().about, Some("List what's here"));
    let nothing_said = ToolUseAsked::of(&nothing_said).unwrap();
    assert_eq!((nothing_said.about, nothing_said.reason, nothing_said.asked_by), (None, None, None));
}

#[test]
fn an_edit_or_a_write_is_given_what_it_would_change_and_another_tool_its_main_input() {
    let edit = json!({ "file_path": "src/lib.rs", "old_string": "a", "new_string": "b" });
    let edit: EventBody = request("Edit", CallKind::Edit, edit, Some("Fix the typo")).into();
    let write = json!({ "file_path": "notes.md", "content": "# Notes\n" });
    let write: EventBody = request("Write", CallKind::Write, write, None).into();
    let fetch: EventBody = request("WebFetch", CallKind::Fetch, json!({ "url": "https://example.com" }), None).into();

    let edit = ToolUseAsked::of(&edit).unwrap();
    let write = ToolUseAsked::of(&write).unwrap();

    assert_eq!((edit.tool.as_str(), edit.about), ("Edit", Some("Fix the typo")));
    assert_eq!(edit.given, Some(Given::Edit(EditFile { path: "src/lib.rs", edits: vec![("a", "b")] })));
    assert_eq!(write.given, Some(Given::Write(WriteFile { path: "notes.md", content: "# Notes\n" })));
    assert_eq!((edit.given.unwrap().path(), write.given.unwrap().path()), (Some("src/lib.rs"), Some("notes.md")));
    let fetch = ToolUseAsked::of(&fetch).unwrap();
    assert_eq!(fetch.given, Some(Given::MainInput("https://example.com".into())));
}

#[test]
fn a_tool_with_no_kind_of_its_own_has_a_readable_name_and_doesnt_say_whats_said_twice() {
    let kind = || CallKind::Other { server: Some("team".into()), tool: "post_message".into() };
    let input = json!({ "channel": "general", "description": "Tell the team" });
    let described_only: EventBody = request("mcp__team__post_message", kind(), input, Some("Tell the team")).into();
    let with_a_path = json!({ "path": "notes.md" });
    let with_an_input: EventBody = request("mcp__team__post_message", kind(), with_a_path, None).into();

    let described_only = ToolUseAsked::of(&described_only).unwrap();
    let with_an_input = ToolUseAsked::of(&with_an_input).unwrap();

    assert_eq!(described_only.tool, "Post message");
    assert_eq!((described_only.about, described_only.given), (Some("Tell the team"), None));
    assert_eq!((with_an_input.about, with_an_input.given), (None, Some(Given::MainInput("notes.md".into()))));
}

#[test]
fn a_request_this_build_doesnt_know_is_a_tools_use_and_questions_a_plan_and_other_events_arent() {
    let asking =
        |asking: Asking| -> EventBody { ApprovalRequest { asking: Some(asking), ..approval_request("a", "X") }.into() };

    assert!(ToolUseAsked::of(&asking(Asking::Unknown)).is_some());
    assert!(ToolUseAsked::of(&asking(Asking::Questions { questions: Vec::new() })).is_none());
    assert!(ToolUseAsked::of(&asking(Asking::Plan { plan: String::new() })).is_none());
    assert!(ToolUseAsked::of(&EventBody::Unknown).is_none());
}

#[test]
fn an_answer_allows_or_denies_by_its_place_and_whats_typed_denies_in_those_words() {
    let body: EventBody = request("Bash", CallKind::Command, json!({ "command": "ls" }), None).into();
    let approval = Approval::of(&body).unwrap();
    let picked = |rows: &[u32]| QuestionAnswer { picked: rows.to_vec(), typed: None };
    let typed = QuestionAnswer { picked: vec![0], typed: Some("Use the full path".into()) };

    assert_eq!(ToolUseAnswer::ALL, [ToolUseAnswer::Allow, ToolUseAnswer::Deny], "the order every client shows");
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    assert_eq!(approval.deciding_tool_use(&picked(&[0])), Some(allow));
    let deny = ApprovalDecision::Deny { message: None, interrupt: false };
    assert_eq!(approval.deciding_tool_use(&picked(&[1])), Some(deny), "with the words left to the adapter");
    let instead = ApprovalDecision::Deny { message: Some("Use the full path".into()), interrupt: false };
    assert_eq!(approval.deciding_tool_use(&typed), Some(instead), "whatever row came with it");
    for neither in [picked(&[]), picked(&[7]), picked(&[0, 1])] {
        assert_eq!(approval.deciding_tool_use(&neither), None, "{neither:?}");
    }
}
