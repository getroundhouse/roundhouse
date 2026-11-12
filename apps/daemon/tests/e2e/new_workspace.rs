//! Making a workspace from what's in the new-workspace dialog, as the hub keeps it, files and all, so a client sends
//! none of them again.

#![cfg(test)]

use client_core::{Client, FileToSend};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::{Effort, ModelName, NewWorkspaceDraftToKeep, PermissionMode, SessionId};
use protocol::prompt::Prompt;

use crate::support::{Stack, read_all, wait_until};

/// Keeps a task in the dialog, with a file, for the stack's repository, planning on Sonnet at low effort.
fn kept_in_the_dialog(stack: &Stack, client: &Client, text: &str) -> NewWorkspaceDraftToKeep {
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let notes = FileToSend::checked("notes.txt", MediaType::Text, b"one\n".to_vec()).unwrap();
    client.new_workspace_draft_file_add(&notes.attachment.id, "notes.txt", MediaType::Text, notes.data).unwrap();
    let draft = NewWorkspaceDraftToKeep {
        repo_id: Some(repo.id),
        text: text.to_string(),
        files: vec![notes.attachment.id],
        model: Some(ModelName::named("sonnet")),
        effort: Some(Effort::Low),
        plan: true,
    };
    client.new_workspace_draft_keep(&draft).unwrap();
    draft
}

/// Each prompt the chat has been sent: what it said, and the files it carried.
fn prompts(client: &Client, session: &SessionId) -> Vec<Prompt<Attachment>> {
    let events = read_all(client, session);
    let prompts = events.into_iter().filter_map(|event| match event.body {
        EventBody::UserPrompt { prompt, .. } => Some(prompt),
        _ => None,
    });
    prompts.collect()
}

#[test]
fn a_workspace_made_from_the_dialog_has_what_it_said_and_the_dialog_is_emptied() {
    let stack = Stack::start();
    let client = stack.client();
    let made_of = kept_in_the_dialog(&stack, &client, "fix the login");

    let made = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    assert_eq!(made.unsent, None);
    let session = &made.session;
    assert_eq!(session.workspace_id, made.workspace.id);
    assert_eq!(
        (&session.model, session.effort, session.permission_mode),
        (&ModelName::named("sonnet"), Effort::Low, PermissionMode::Plan)
    );
    wait_until("the prompt", || !prompts(&client, &session.id).is_empty());
    let [Prompt { text, files }] = prompts(&client, &session.id).try_into().unwrap();
    assert_eq!(text, "fix the login");
    assert_eq!(
        files.iter().map(|file| (&file.id, file.name.as_str())).collect::<Vec<_>>(),
        [(&made_of.files[0], "notes.txt")]
    );
    let kept = client.records_list().unwrap().new_workspace_draft.unwrap();
    assert_eq!((kept.text.as_str(), kept.files.as_slice(), kept.plan), ("", [].as_slice(), false));
    assert_eq!(
        (kept.repo_id, kept.model, kept.effort),
        (made_of.repo_id, made_of.model, made_of.effort),
        "the next one's made the same way"
    );
    assert!(client.new_workspace_draft_file_read(&made_of.files[0]).is_err(), "the dialog's copy is let go of");
}

/// Such as in another client, as the workspace was being made.
#[test]
fn whats_typed_in_the_dialog_since_is_kept() {
    let stack = Stack::start();
    let client = stack.client();
    let made_of = kept_in_the_dialog(&stack, &client, "fix the login");
    let since = NewWorkspaceDraftToKeep { text: "fix the logout".to_string(), ..made_of.clone() };
    client.new_workspace_draft_keep(&since).unwrap();

    let made = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    wait_until("the prompt", || !prompts(&client, &made.session.id).is_empty());
    assert_eq!(prompts(&client, &made.session.id)[0].text, "fix the login");
    let kept = client.records_list().unwrap().new_workspace_draft.unwrap();
    assert_eq!(kept.text, "fix the logout");
    assert_eq!(client.new_workspace_draft_file_read(&made_of.files[0]).unwrap(), b"one\n", "it still names its file");
}

#[test]
fn sent_again_it_makes_nothing_more() {
    let stack = Stack::start();
    let client = stack.client();
    let made_of = kept_in_the_dialog(&stack, &client, "fix the login");
    let first = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    let again = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    assert_eq!((&again.workspace.id, &again.session.id), (&first.workspace.id, &first.session.id));
    assert_eq!(again.unsent, None);
    wait_until("the prompt", || !prompts(&client, &first.session.id).is_empty());
    assert_eq!(prompts(&client, &first.session.id).len(), 1);
    assert_eq!(client.workspace_list(false).unwrap().len(), 1);
}

#[test]
fn with_nothing_typed_the_workspace_and_its_chat_are_made_and_nothing_is_sent() {
    let stack = Stack::start();
    let client = stack.client();
    let made_of =
        NewWorkspaceDraftToKeep { text: String::new(), files: Vec::new(), ..kept_in_the_dialog(&stack, &client, "") };
    client.new_workspace_draft_keep(&made_of).unwrap();

    let made = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    assert_eq!(made.session.permission_mode, PermissionMode::Plan);
    assert_eq!(prompts(&client, &made.session.id), []);
    assert!(!client.records_list().unwrap().new_workspace_draft.unwrap().plan, "the dialog is emptied, plan mode too");
}
