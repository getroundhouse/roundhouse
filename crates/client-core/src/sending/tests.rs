// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;

use protocol::attachment::{Attachment, MediaType};
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, queue};
use protocol::frame;
use protocol::prompt::Prompt;

use super::*;
use crate::stand_in_hub::{self, Does};

/// Answers a hello, then hangs up, so every call after it fails.
fn connected_to_a_hub_that_goes_away(socket: &Path) -> Client {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        BufReader::new(stream.try_clone().unwrap()).lines().next();
        let hello = ClientResponse::Hello { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: "a".to_string() };
        let outcome = ReplyOutcome::Response(hello);
        stream.write_all(&frame::encode(&HubFrame::Reply { id: 1, outcome }).unwrap()).unwrap();
    });
    Client::connect(socket, "test").unwrap()
}

#[test]
fn a_prompt_the_hub_never_takes_comes_back_with_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let client = connected_to_a_hub_that_goes_away(&dir.path().join("hub.sock"));
    let log = FileToSend::checked("build.log", MediaType::Text, b"error at line 42".to_vec()).unwrap();
    let written = Written::new(Prompt::carrying("why?", vec![log.clone()]));

    let Err(NotTaken { error, prompt }) = client.start_queueing(&"s".into(), written).wait() else {
        panic!("the hub took it")
    };

    assert!(matches!(error, ClientError::Disconnected | ClientError::Io(_)), "{error:?}");
    assert_eq!(prompt.text, "why?");
    assert_eq!(
        prompt.files.iter().map(|file| (&file.attachment, &file.data)).collect::<Vec<_>>(),
        [(&log.attachment, &log.data)]
    );
    assert!(client.queueing(&stand_in_hub::session("s")).is_empty(), "it's handed back, not shown as queued");
}

/// So the queue shows a prompt from the moment it's queued, before the hub has it, and shows it once.
#[test]
fn a_prompt_being_queued_shows_until_the_hub_answers_and_not_again_once_it_s_on_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let (answer, answered) = flume::bounded::<()>(0);
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        let ClientRequest::SessionQueue(queue::SessionQueue { id, .. }) = request else { return Does::Nothing };
        let _ = answered.recv();
        Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Session(stand_in_hub::session(id.as_str())))))
    });
    let (session, written) = (stand_in_hub::session("s"), Written::new(Prompt::new("next")));
    let shown = written.on_its_way();

    let queueing = client.start_queueing(&session.id, written);

    assert_eq!(client.queueing(&session), std::slice::from_ref(&shown));
    assert!(client.queueing(&stand_in_hub::session("t")).is_empty(), "only in its own chat");
    let mut on_record = session.clone();
    on_record.queued.push(shown);
    assert!(client.queueing(&on_record).is_empty(), "the record shows it");
    answer.send(()).unwrap();
    queueing.wait().unwrap();
    assert!(client.queueing(&session).is_empty(), "the hub has answered for it");
}

/// A hub that takes every file and prompt, sent or queued, after saying what it was, except that it says it hasn't got
/// a file the first `missing` prompts carry.
fn hub_that_says(socket: &Path, mut missing: usize) -> (Client, flume::Receiver<String>) {
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(socket, move |request| {
        // Only what these tests send is answered.
        let (prompt, taken) = if matches!(request, ClientRequest::SessionPrompt { .. }) {
            ("prompt", ClientResponse::PromptRecorded { seq: 1, duplicate: false })
        } else if let ClientRequest::SessionQueue(queue::SessionQueue { id, .. }) = &request {
            ("queued", ClientResponse::Session(stand_in_hub::session(id.as_str())))
        } else if let ClientRequest::AttachmentAdd { id, name, media_type, data, .. } = request {
            let _ = said.send(format!("add {id}"));
            let size_bytes = data.into_vec().len() as u64;
            let added = ClientResponse::Attachment(Attachment { id, name, media_type, size_bytes });
            return Does::Answers(Box::new(ReplyOutcome::Response(added)));
        } else {
            return Does::Nothing;
        };
        if missing > 0 {
            missing -= 1;
            let _ = said.send(format!("{prompt}, not taken"));
            return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::NotFound, "no attachment"))));
        }
        let _ = said.send(prompt.to_string());
        Does::Answers(Box::new(ReplyOutcome::Response(taken)))
    });
    (client, heard)
}

fn log() -> FileToSend {
    FileToSend::checked("build.log", MediaType::Text, b"error at line 42".to_vec()).unwrap()
}

/// So what's shown while it's on its way is the prompt the hub records, under the same ID, which keeps it one item on
/// screen from written to recorded.
#[test]
fn a_prompt_on_its_way_is_the_one_written_with_the_hub_s_record_of_each_file() {
    let log = log();
    let written = Written::new(Prompt::carrying("why?", vec![log.clone()]));

    let shown = written.on_its_way();

    assert_eq!(shown.client_msg_id, written.client_msg_id);
    assert_eq!(shown.prompt, Prompt::carrying("why?", vec![log.attachment]));
    assert_ne!(Written::new(Prompt::new("why?")).client_msg_id, written.client_msg_id, "each its own");
}

/// As one the app knows and its hub doesn't, which the hub refuses as it would a request it doesn't know.
#[test]
fn a_file_of_a_kind_the_hub_doesnt_know_goes_with_the_prompt_as_one_that_goes_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        let outcome = if matches!(request, ClientRequest::SessionPrompt { .. }) {
            let _ = said.send("prompt".to_string());
            ReplyOutcome::Response(ClientResponse::PromptRecorded { seq: 1, duplicate: false })
        } else if let ClientRequest::AttachmentAdd { id, name, media_type, data, .. } = request {
            if media_type != MediaType::Other {
                let _ = said.send(format!("refused {}", media_type.mime()));
                return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "no kind"))));
            }
            let _ = said.send(format!("add {}", media_type.mime()));
            let size_bytes = data.into_vec().len() as u64;
            ReplyOutcome::Response(ClientResponse::Attachment(Attachment { id, name, media_type, size_bytes }))
        } else {
            return Does::Nothing;
        };
        Does::Answers(Box::new(outcome))
    });

    client.send_prompt(&"s".into(), &Written::new(Prompt::carrying("why?", vec![log()]))).unwrap();

    let said = ["refused text/plain", "add application/octet-stream", "prompt"];
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), said);
}

#[test]
fn a_file_this_connection_gave_the_hub_isnt_sent_again_with_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub_that_says(&dir.path().join("hub.sock"), 0);
    let log = log();
    let attachment = &log.attachment;
    client
        .attachment_add(&"s".into(), &attachment.id, &attachment.name, attachment.media_type, log.data.clone())
        .unwrap();

    client.send_prompt(&"s".into(), &Written::new(Prompt::carrying("why?", vec![log.clone()]))).unwrap();

    let id = &log.attachment.id;
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), [format!("add {id}"), "prompt".to_string()]);
}

#[test]
fn a_file_the_hub_turns_out_not_to_have_is_sent_again_and_the_prompt_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub_that_says(&dir.path().join("hub.sock"), 1);
    let log = log();
    let attachment = &log.attachment;
    client
        .attachment_add(&"s".into(), &attachment.id, &attachment.name, attachment.media_type, log.data.clone())
        .unwrap();

    client.send_prompt(&"s".into(), &Written::new(Prompt::carrying("why?", vec![log.clone()]))).unwrap();

    let added = format!("add {}", log.attachment.id);
    let said = [added.clone(), "prompt, not taken".to_string(), added, "prompt".to_string()];
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), said);
}

#[test]
fn a_queued_prompts_file_the_hub_turns_out_not_to_have_is_sent_again_and_the_prompt_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub_that_says(&dir.path().join("hub.sock"), 1);
    let log = log();
    let attachment = &log.attachment;
    client
        .attachment_add(&"s".into(), &attachment.id, &attachment.name, attachment.media_type, log.data.clone())
        .unwrap();

    client.start_queueing(&"s".into(), Written::new(Prompt::carrying("why?", vec![log.clone()]))).wait().unwrap();

    let added = format!("add {}", log.attachment.id);
    let said = [added.clone(), "queued, not taken".to_string(), added, "queued".to_string()];
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), said);
}
