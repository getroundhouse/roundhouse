//! What's typed and not sent, in a chat's composer, in answer to its question, in a comment on a workspace's diff and
//! in the new-workspace dialog, which the hub keeps for whoever opens it next.

#![cfg(test)]

use client_core::records::HubRecords;
use client_core::{Drafts, FileToSend, RecordsFeed};
use protocol::attachment::MediaType;
use protocol::model::{
    AnswerDraft, AnswerSoFar, CommentDraft, DiffSide, Draft, NewDraft, SessionId, SessionStatus, WorkspaceDraftToKeep,
    WorkspaceId,
};

use crate::support::{Stack, wait_for_setup, wait_until};

/// What client-core's copy says is typed in this session.
fn draft_in(feed: &RecordsFeed, id: &SessionId) -> Option<String> {
    feed.read(|records: &HubRecords| {
        records.sessions().values().flatten().find(|s| &s.id == id).map(|s| s.draft.text.clone())
    })
}

#[test]
fn a_draft_is_kept_across_a_restart_and_every_client_hears_of_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);
    let (_, session) = stack.workspace(&client);

    let kept = client.session_draft(&session.id, &NewDraft::new("half a thought")).unwrap();

    assert_eq!(kept.draft.text, "half a thought");
    assert_eq!(kept.status, SessionStatus::Idle, "keeping a draft doesn't start the agent");
    wait_until("the feed to say what's typed", || draft_in(&feed, &session.id).as_deref() == Some("half a thought"));
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().draft.text, "half a thought");
    assert_eq!(client.session_draft(&session.id, &NewDraft::default()).unwrap().draft, Draft::default());
}

#[test]
fn what_a_client_types_is_kept_once_it_quits() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let drafts = Drafts::start(&client);

    for text in ["h", "ha", "half", "half a", "half a thought"] {
        drafts.typed(&session.id, NewDraft::new(text));
    }
    drafts.flushed().recv().unwrap();

    assert_eq!(client.session_get(&session.id).unwrap().draft.text, "half a thought");
}

#[test]
fn a_drafts_files_are_kept_across_a_restart_until_its_let_go_of_them() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let drafts = Drafts::start(&client);
    let log = FileToSend::checked("build.log", MediaType::Text, b"error at line 42\n".to_vec()).unwrap();
    let never_given = FileToSend::checked("other.log", MediaType::Text, b"a line\n".to_vec()).unwrap();

    drafts.attach(&session.id, log.clone());
    let files = vec![log.attachment.id.clone(), never_given.attachment.id.clone()];
    drafts.set(&session.id, NewDraft { text: "look".to_string(), files });
    drafts.flushed().recv().unwrap();

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let (id, file) = (&session.id, &log.attachment.id);
    let kept = client.session_get(id).unwrap().draft;
    assert_eq!(kept.files, std::slice::from_ref(&log.attachment), "the hub leaves out one it wasn't given");
    assert_eq!(client.attachment_read(id, file).unwrap(), log.data);
    client.attachment_remove(id, file).unwrap();
    assert!(client.attachment_read(id, file).is_ok(), "the hub keeps a file the draft names");
    client.session_draft(id, &NewDraft::new("look")).unwrap();
    client.attachment_remove(id, file).unwrap();
    assert!(client.attachment_read(id, file).is_err());
}

/// The comment being written on the workspace's diff, as client-core's copy has it.
fn comment_in(feed: &RecordsFeed, id: &WorkspaceId) -> Option<String> {
    let workspace = feed.read(|records: &HubRecords| records.workspaces().into_iter().find(|w| &w.id == id));
    workspace.and_then(|w| w.comment_draft).map(|draft| draft.text)
}

#[test]
fn a_comment_being_written_and_an_answer_being_typed_are_kept_across_a_restart_and_every_client_hears_of_them() {
    let mut stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);
    let (workspace, session) = stack.workspace(&client);
    let comment =
        CommentDraft { path: "README.md".into(), side: DiffSide::New, line: 1, editing: None, text: "why?".into() };
    let so_far = AnswerSoFar { picked: vec![0], typed: "the other one".into(), other: true };
    let answer = AnswerDraft { approval_id: "q".into(), answers: vec![so_far] };
    // So only the notices of what's kept can bring it, rather than the feed's first list, or the notice that setup
    // has finished.
    let setup = wait_for_setup(&client, &workspace.id);
    let caught_up = || {
        let held = feed.read(|records: &HubRecords| records.workspaces().into_iter().find(|w| w.id == workspace.id));
        held.is_some_and(|w| w.setup == setup)
    };
    wait_until("client-core to hear setup has finished", caught_up);

    let commented = client.workspace_comment_draft(&workspace.id, Some(&comment)).unwrap();
    let answered = client.session_answer_draft(&session.id, Some(&answer)).unwrap();

    assert_eq!(commented.comment_draft.as_ref(), Some(&comment));
    assert_eq!(answered.answer_draft.as_ref(), Some(&answer));
    assert_eq!(answered.status, SessionStatus::Idle, "keeping an answer doesn't start the agent");
    wait_until("the feed to say what's being written", || comment_in(&feed, &workspace.id).as_deref() == Some("why?"));
    let answer_in = || {
        feed.read(|records: &HubRecords| records.sessions().values().flatten().find(|s| s.id == session.id).cloned())
            .and_then(|s| s.answer_draft)
    };
    wait_until("the feed to say what's typed in answer", || answer_in().as_ref() == Some(&answer));
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().answer_draft, Some(answer));
    let workspaces = client.workspace_list(false).unwrap();
    let kept = workspaces.into_iter().find(|w| w.id == workspace.id).unwrap();
    assert_eq!(kept.comment_draft, Some(comment));
    assert_eq!(client.workspace_comment_draft(&workspace.id, None).unwrap().comment_draft, None);
    assert_eq!(client.session_answer_draft(&session.id, None).unwrap().answer_draft, None);
}

#[test]
fn whats_in_the_new_workspace_dialog_is_kept_with_its_files_across_a_restart_and_every_client_hears_of_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);
    let dialog_in = || feed.read(|records: &HubRecords| records.workspace_draft()).map(|draft| draft.text);
    // So only the notice of what's kept can bring it, rather than the feed's first list.
    wait_until("client-core's first list", || dialog_in().is_some());
    let notes = FileToSend::checked("notes.txt", MediaType::Text, b"one\n".to_vec()).unwrap();
    let file = &notes.attachment.id;
    let added = client.workspace_draft_file_add(file, "notes.txt", MediaType::Text, notes.data.clone()).unwrap();
    let draft = WorkspaceDraftToKeep {
        text: "fix the login".into(),
        files: vec![file.clone()],
        plan: true,
        ..WorkspaceDraftToKeep::default()
    };

    let kept = client.workspace_draft_keep(&draft).unwrap();

    assert_eq!((kept.text.as_str(), kept.files.as_slice(), kept.plan), ("fix the login", [added].as_slice(), true));
    wait_until("the feed to say what's in the dialog", || dialog_in().as_deref() == Some("fix the login"));
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.records_list().unwrap().workspace_draft, Some(kept));
    assert_eq!(client.workspace_draft_file_read(file).unwrap(), notes.data);
    client.workspace_draft_file_remove(file).unwrap();
    assert!(client.workspace_draft_file_read(file).is_ok(), "the hub keeps a file the draft names");
    client.workspace_draft_keep(&WorkspaceDraftToKeep::default()).unwrap();
    client.workspace_draft_file_remove(file).unwrap();
    assert!(client.workspace_draft_file_read(file).is_err());
}
