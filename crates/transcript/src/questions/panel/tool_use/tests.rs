use client_core::Approval;
use protocol::event::{ApprovalDecision, Asking, CallKind, EventBody, QuestionAnswer};
use protocol::fixtures::{ApprovalRequest, approval_request};
use serde_json::{Value, json};

use super::{Shown, ToolUse};

/// What's shown of what the tool was given, as text, or for a change, its file.
fn given(asked: &ToolUse) -> Option<&str> {
    asked.given.as_ref().map(|given| match given {
        Shown::Text(text) => text.as_str(),
        Shown::Change(change) => change.path.as_str(),
    })
}

fn asked(tool: &str, kind: CallKind, input: Value, description: Option<&str>, reason: Option<&str>) -> ToolUse {
    let body: EventBody = ApprovalRequest {
        input,
        description: description.map(str::to_string),
        asking: Some(Asking::ToolUse { call: Some(kind) }),
        reason: reason.map(str::to_string),
        ..approval_request("a", tool)
    }
    .into();
    ToolUse::read(&body, Approval::of(&body).unwrap().into_owned(), true).unwrap()
}

fn command(description: Option<&str>) -> ToolUse {
    asked("Bash", CallKind::Command, json!({ "command": "ls -la" }), description, Some("It can't be checked"))
}

#[test]
fn a_command_is_asked_by_what_its_for_and_shown_whole_with_why() {
    let asked = command(Some("List what's here"));

    assert_eq!(asked.question.header.as_deref(), Some("Bash"));
    assert_eq!(asked.question.text, "List what's here");
    let rows: Vec<_> =
        asked.question.options.iter().map(|row| (row.label.as_str(), row.description.as_deref())).collect();
    let expected = [("Allow", Some("Runs it this once")), ("Deny", Some("The agent is told it can't, and carries on"))];
    assert_eq!(rows, expected);
    assert!(!asked.question.multiple);
    assert_eq!((given(&asked), asked.reason.as_deref()), (Some("ls -la"), Some("It can't be checked")));
}

#[test]
fn what_nobody_described_still_asks_something() {
    let edit = json!({ "file_path": "src/lib.rs", "old_string": "a", "new_string": "b" });
    let edit = |description| asked("Edit", CallKind::Edit, edit.clone(), description, None);
    let write = asked("Write", CallKind::Write, json!({ "file_path": "notes.md", "content": "Hi" }), None, None);
    let fetch = asked("WebFetch", CallKind::Fetch, json!({ "url": "https://example.com" }), None, None);
    let bare = asked("Ask", CallKind::Unknown, json!({}), None, None);

    assert_eq!(command(None).question.text, "Run this command?");
    let (described, plain) = (edit(Some("Fix the typo")), edit(None));
    assert_eq!((described.question.text.as_str(), given(&described)), ("Fix the typo", Some("src/lib.rs")));
    assert_eq!((plain.question.text.as_str(), given(&plain)), ("Make this edit?", Some("src/lib.rs")));
    assert!(matches!(plain.given, Some(Shown::Change(_))), "shown as what it would change");
    assert_eq!((write.question.text.as_str(), given(&write)), ("Write this file?", Some("notes.md")));
    assert_eq!((fetch.question.text.as_str(), given(&fetch)), ("https://example.com", None), "said once");
    assert_eq!(plain.question.options[0].description.as_deref(), Some("This once"), "nothing runs");
    assert_eq!((bare.question.text.as_str(), given(&bare), bare.reason.as_deref()), ("Use this tool?", None, None));
}

#[test]
fn the_answer_is_read_as_client_core_reads_it() {
    let asked = command(None);
    let typed = QuestionAnswer { picked: vec![0], typed: Some("Use the full path".into()) };

    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    assert_eq!(asked.decision(&QuestionAnswer { picked: vec![0], typed: None }), Some(allow));
    let instead = ApprovalDecision::Deny { message: Some("Use the full path".into()), interrupt: false };
    assert_eq!(asked.decision(&typed), Some(instead));
}
