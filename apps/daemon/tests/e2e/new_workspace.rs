//! Making a workspace from what's in the new-workspace dialog, as the hub keeps it, files and all, so a client sends
//! none of them again. One example, through a client, a hub and a worker: the hub's own tests, in its
//! `hub/new_workspace`, have what it does with each thing a dialog can say.

#![cfg(test)]

use client_core::{Client, FileToSend};
use protocol::attachment::{Attachment, MediaType};
use protocol::capability::settings::Effort;
use protocol::event::EventBody;
use protocol::model::{ModelName, NewWorkspaceDraftToKeep, SessionId};
use protocol::prompt::Prompt;

use crate::support::{Stack, read_all, wait_until};

/// Keeps a task in the dialog, with a file, for the stack's repository, planning on Sonnet at low effort.
fn kept_in_the_dialog(stack: &Stack, client: &Client) -> NewWorkspaceDraftToKeep {
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let notes = FileToSend::checked("notes.txt", MediaType::Text, b"one\n".to_vec()).unwrap();
    client.new_workspace_draft_file_add(&notes.attachment.id, "notes.txt", MediaType::Text, notes.data).unwrap();
    let draft = NewWorkspaceDraftToKeep {
        agent: None,
        repo_id: Some(repo.id),
        text: "fix the login".to_string(),
        files: vec![notes.attachment.id],
        model: Some(ModelName::named("sonnet")),
        effort: Some(Effort::Low),
        mode: Some(protocol::fixtures::plan_mode()),
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
    let made_of = kept_in_the_dialog(&stack, &client);

    let made = client.new_workspace_create(&made_of, "make it".into()).unwrap();

    assert_eq!(made.unsent, None);
    let session = &made.session;
    assert_eq!(session.workspace_id, made.workspace.id);
    assert_eq!(
        (&session.model, session.effort.clone(), session.mode),
        (&ModelName::named("sonnet"), Effort::Low, protocol::fixtures::plan_mode())
    );
    wait_until("the prompt", || !prompts(&client, &session.id).is_empty());
    let [Prompt { text, files }] = prompts(&client, &session.id).try_into().unwrap();
    assert_eq!(text, "fix the login");
    assert_eq!(
        files.iter().map(|file| (&file.id, file.name.as_str())).collect::<Vec<_>>(),
        [(&made_of.files[0], "notes.txt")]
    );
    let kept = client.records_list().unwrap().new_workspace_draft.unwrap();
    assert_eq!((kept.text.as_str(), kept.files.as_slice(), kept.mode), ("", [].as_slice(), None));
    assert_eq!(
        (kept.repo_id, kept.model, kept.effort),
        (made_of.repo_id, made_of.model, made_of.effort),
        "the next one's made the same way"
    );
    assert!(client.new_workspace_draft_file_read(&made_of.files[0]).is_err(), "the dialog's copy is let go of");
}
