//! Against a stand-in hub, which can be one too old to make a workspace from what it keeps of the dialog.
#![cfg(unix)]

use std::time::Duration;

use client_core::stand_in_hub::{self, Does};
use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, agent, attachments, drafts};
use protocol::fixtures;
use protocol::model::{AgentKind, AttachmentId, Draft, Effort, ModelName, RepoId};

use super::*;

/// A hub that says what it was asked, in order, and answers as `answer` says, or with nothing.
fn hub_that_says(
    dir: &tempfile::TempDir,
    mut answer: impl FnMut(&ClientRequest) -> Option<ReplyOutcome> + Send + 'static,
) -> (Client, flume::Receiver<String>) {
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        let _ = said.send(asked(&request));
        match answer(&request) {
            Some(outcome) => Does::Answers(Box::new(outcome)),
            None => Does::Nothing,
        }
    });
    (client, heard)
}

/// What a request asked, in a word or two.
fn asked(request: &ClientRequest) -> String {
    match request {
        ClientRequest::NewWorkspaceCreate(_) => "create from the dialog".to_string(),
        ClientRequest::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id }) => format!("read {id}"),
        ClientRequest::WorkspaceCreate(_) => "workspace".to_string(),
        ClientRequest::SessionEnsure(_) => "chat".to_string(),
        ClientRequest::AttachmentAdd(attachments::AttachmentAdd { id, .. }) => format!("add {id}"),
        ClientRequest::SessionPrompt(agent::SessionPrompt { prompt, .. }) => {
            let carrying: Vec<&str> = prompt.files.iter().map(AttachmentId::as_str).collect();
            format!("prompt carrying {}", carrying.join(", "))
        }
        other => format!("{other:?}"),
    }
}

/// What a hub too old to make a workspace from what it keeps answers each step of making one a step at a time with,
/// reading back each of the dialog's files as `read` says.
fn too_old(request: &ClientRequest, read: &impl Fn(&AttachmentId) -> ReplyOutcome) -> Option<ReplyOutcome> {
    let answer = match request {
        ClientRequest::NewWorkspaceCreate(_) => {
            return Some(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "unknown request")));
        }
        ClientRequest::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id }) => return Some(read(id)),
        ClientRequest::WorkspaceCreate(_) => ClientResponse::Workspace(fixtures::workspace("w")),
        ClientRequest::SessionEnsure(_) => ClientResponse::Session(fixtures::session("s")),
        ClientRequest::AttachmentAdd(attachments::AttachmentAdd { id, name, media_type, data, .. }) => {
            let size_bytes = data.len() as u64;
            ClientResponse::Attachment(Attachment {
                id: id.clone(),
                name: name.clone(),
                media_type: *media_type,
                size_bytes,
            })
        }
        ClientRequest::SessionPrompt(_) => {
            ClientResponse::PromptRecorded(agent::PromptRecorded { seq: 1, duplicate: false })
        }
        _ => return None,
    };
    Some(ReplyOutcome::Response(answer))
}

/// A file the hub keeps for the dialog, which the dialog hadn't read back.
fn on_the_hub() -> Attachment {
    Attachment {
        id: AttachmentId::from("kept"),
        name: "kept.txt".to_string(),
        media_type: MediaType::Text,
        size_bytes: 4,
    }
}

/// What Create asks for, with a file of the dialog's and one still on the hub, and the model, effort and mode the
/// stand-in's chat has.
fn request() -> Request {
    let here = FileToSend::checked("here.txt", MediaType::Text, b"here".to_vec()).unwrap();
    let chosen = (Some(ModelName::named("opus")), Some(Effort::Xhigh));
    let prompt = Prompt::carrying("say:hi", vec![here]);
    let request = Request::new(RepoId::from("r"), prompt, AgentKind::Claude, chosen, PermissionMode::FullAccess);
    let mut made_of = request.made_of.clone();
    made_of.files.insert(0, on_the_hub().id);
    Request { coming_back: vec![on_the_hub()], made_of, ..request }
}

#[test]
fn a_hub_too_old_to_make_it_is_sent_it_a_step_at_a_time_with_the_files_it_keeps_first() {
    let dir = tempfile::tempdir().unwrap();
    let read = |_: &AttachmentId| {
        let data = AttachmentBytes::new(b"kept".to_vec());
        ReplyOutcome::Response(ClientResponse::AttachmentData(attachments::AttachmentData { data }))
    };
    let (client, heard) = hub_that_says(&dir, move |request| too_old(request, &read));
    let request = request();
    let (made_of, here) = (request.made_of.clone(), request.prompt.files[0].attachment.id.clone());

    let made = make(&client, request);

    assert!(made.unsent.is_none(), "{:?}", made.unsent.map(|unsent| unsent.error));
    assert!(made.workspace.is_some() && made.session.is_some());
    let said: Vec<String> = heard.try_iter().collect();
    assert_eq!(
        said,
        [
            "create from the dialog".to_string(),
            "read kept".to_string(),
            "workspace".to_string(),
            "chat".to_string(),
            "add kept".to_string(),
            format!("add {here}"),
            format!("prompt carrying kept, {here}"),
        ]
    );
    assert_eq!(made.dialog_to_empty, Some(made_of), "the dialog is to let go of what it took");
}

/// So nothing is made without it, and it can be read back again.
#[test]
fn a_file_the_hub_kept_that_cant_be_read_back_gives_all_of_it_back_before_anything_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let read = |_: &AttachmentId| ReplyOutcome::Error(ApiError::new(ErrorCode::NotFound, "no attachment kept"));
    let (client, heard) = hub_that_says(&dir, move |request| too_old(request, &read));
    let request = request();
    let here = request.prompt.files[0].attachment.id.clone();

    let made = make(&client, request);

    assert!(made.workspace.is_none() && made.dialog_to_empty.is_none());
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), ["create from the dialog", "read kept"]);
    let unsent = made.unsent.expect("it's given back");
    assert_eq!(unsent.failed, Step::Workspace);
    let given_back = unsent.given_back.expect("all of it");
    assert_eq!(given_back.prompt.text, "say:hi");
    assert_eq!(given_back.prompt.files.iter().map(|file| &file.attachment.id).collect::<Vec<_>>(), [&here]);
    assert_eq!(given_back.coming_back, [on_the_hub()]);
}

/// A hub that makes it, and couldn't send the prompt, which it put in the chat's composer if `in_composer`, and whose
/// chat couldn't be given what was chosen if `unchosen`.
fn made_by_the_hub(in_composer: bool, unchosen: bool) -> Made {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub_that_says(&dir, move |request| {
        let ClientRequest::NewWorkspaceCreate(_) = request else { return None };
        let mut session = fixtures::session("s");
        if in_composer {
            session.draft = Draft { text: "say:hi".to_string(), files: vec![on_the_hub()] };
        }
        let unchosen = unchosen.then(|| ApiError::new(ErrorCode::InvalidRequest, "not that effort"));
        let unsent = Some(UnsentPrompt { error: ApiError::new(ErrorCode::Internal, "it went wrong"), in_composer });
        let made = NewWorkspaceMade { workspace: fixtures::workspace("w"), session, unchosen, unsent };
        Some(ReplyOutcome::Response(ClientResponse::NewWorkspaceMade(Box::new(made))))
    });
    make(&client, request())
}

/// One the hub couldn't give the chat whole is still in the dialog, where it goes back, with the file the dialog hadn't
/// read back.
#[test]
fn a_prompt_the_hub_made_it_with_and_couldnt_send_is_given_back_to_the_dialog_unless_its_in_the_composer() {
    let in_composer = made_by_the_hub(true, false).unsent.expect("it wasn't sent");
    assert!(in_composer.given_back.is_none(), "the composer has it");
    assert_eq!(in_composer.failed, Step::Prompt);

    let in_the_dialog = made_by_the_hub(false, false).unsent.expect("it wasn't sent");
    let given_back = in_the_dialog.given_back.expect("it's given back");
    assert!(given_back.in_the_dialog);
    assert_eq!(given_back.prompt.text, "say:hi");
    assert_eq!(given_back.prompt.files.len(), 1, "the file the dialog had");
    assert_eq!(given_back.coming_back, [on_the_hub()]);
}

/// Its chat couldn't be given the model, effort and mode chosen, which is what the hub says went wrong.
#[test]
fn a_chat_the_hub_couldnt_give_what_was_chosen_says_so() {
    let unchosen = made_by_the_hub(true, true).unsent.expect("it wasn't sent");

    assert_eq!(unchosen.failed, Step::Model);
    assert!(unchosen.error.to_string().contains("not that effort"), "{}", unchosen.error);
}

/// The hub may have made it before the connection went, so it's not said to have been refused.
#[test]
fn a_hub_that_goes_away_while_making_it_isnt_taken_for_one_that_refused() {
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |request| match request {
        ClientRequest::NewWorkspaceCreate(_) => Does::HangsUp,
        _ => Does::Nothing,
    });

    let made = make(&client, request());

    let unsent = made.unsent.expect("it's given back");
    assert_eq!(unsent.failed, Step::Answer);
    assert!(unsent.given_back.is_some_and(|given_back| given_back.prompt.text == "say:hi"));
}

/// So the hub makes it from what was asked for, and has the files the dialog was given, not from what it kept before.
#[test]
fn the_hub_is_asked_to_make_it_only_once_it_keeps_what_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let read = |_: &AttachmentId| {
        let data = AttachmentBytes::new(b"kept".to_vec());
        ReplyOutcome::Response(ClientResponse::AttachmentData(attachments::AttachmentData { data }))
    };
    let (client, heard) = hub_that_says(&dir, move |request| too_old(request, &read));
    let (kept, keeping) = flume::bounded(1);
    let request = Request { kept: Some(keeping), ..request() };

    let making = std::thread::spawn(move || make(&client, request));

    let before = heard.recv_timeout(Duration::from_millis(200));
    assert!(before.is_err(), "nothing is asked before it's kept, but {before:?} was");
    kept.send(()).unwrap();
    assert_eq!(heard.recv_timeout(Duration::from_secs(10)).unwrap(), "create from the dialog");
    assert!(making.join().unwrap().unsent.is_none());
}
