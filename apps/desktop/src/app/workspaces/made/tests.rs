//! Against a stand-in hub.
#![cfg(unix)]

use std::time::Duration;

use client_core::stand_in_hub::{self, Does};
use protocol::attachment::{Attachment, MediaType};
use protocol::capability::settings::Effort;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::fixtures;
use protocol::model::{AgentKind, AttachmentId, Draft, ModelName, RepoId};

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
        other => format!("{other:?}"),
    }
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
    let request = Request::new(RepoId::from("r"), prompt, AgentKind::Claude, chosen, None);
    let mut made_of = request.made_of.clone();
    made_of.files.insert(0, on_the_hub().id);
    Request { coming_back: vec![on_the_hub()], made_of, ..request }
}

/// How a hub that made it answers, with `unsent` and `unchosen` as it says, and a chat whose composer holds the prompt
/// if `in_composer`.
fn made(in_composer: bool, unsent: Option<UnsentPrompt>, unchosen: Option<ApiError>) -> ReplyOutcome {
    let mut session = fixtures::session("s");
    if in_composer {
        session.draft = Draft { text: "say:hi".to_string(), files: vec![on_the_hub()] };
    }
    let made = NewWorkspaceMade { workspace: fixtures::workspace("w"), session, unchosen, unsent };
    ReplyOutcome::Response(ClientResponse::NewWorkspaceMade(Box::new(made)))
}

/// A hub that makes it, and couldn't send the prompt, which it put in the chat's composer if `in_composer`, and whose
/// chat couldn't be given what was chosen if `unchosen`.
fn made_by_the_hub(in_composer: bool, unchosen: bool) -> Made {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub_that_says(&dir, move |request| {
        let ClientRequest::NewWorkspaceCreate(_) = request else { return None };
        let unchosen = unchosen.then(|| ApiError::new(ErrorCode::InvalidRequest, "not that effort"));
        let unsent = Some(UnsentPrompt { error: ApiError::new(ErrorCode::Internal, "it went wrong"), in_composer });
        Some(made(in_composer, unsent, unchosen))
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
    let (client, heard) = hub_that_says(&dir, |request| {
        matches!(request, ClientRequest::NewWorkspaceCreate(_)).then(|| made(false, None, None))
    });
    let (kept, keeping) = flume::bounded(1);
    let request = Request { kept: Some(keeping), ..request() };

    let making = std::thread::spawn(move || make(&client, request));

    let before = heard.recv_timeout(Duration::from_millis(200));
    assert!(before.is_err(), "nothing is asked before it's kept, but {before:?} was");
    kept.send(()).unwrap();
    assert_eq!(heard.recv_timeout(Duration::from_secs(10)).unwrap(), "create from the dialog");
    assert!(making.join().unwrap().unsent.is_none());
}
