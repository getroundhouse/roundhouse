// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;

use protocol::attachment::{Attachment, MediaType};
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::frame;

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
    let written = Written { client_msg_id: "m".into(), text: "why?".to_string(), files: vec![log.clone()] };

    let Err(NotTaken { error, text, files }) = client.start_queueing(&"s".into(), written).wait() else {
        panic!("the hub took it")
    };

    assert!(matches!(error, ClientError::Disconnected | ClientError::Io(_)), "{error:?}");
    assert_eq!(text, "why?");
    assert_eq!(
        files.iter().map(|file| (&file.attachment, &file.data)).collect::<Vec<_>>(),
        [(&log.attachment, &log.data)]
    );
}

/// A hub that takes every file and prompt, sent or queued, after saying what it was, except that it says it hasn't got
/// a file the first `missing` prompts carry.
fn hub_that_says(socket: &Path, mut missing: usize) -> (Client, flume::Receiver<String>) {
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(socket, move |request| {
        // Only what these tests send is answered.
        let (prompt, taken) = if matches!(request, ClientRequest::SessionPrompt { .. }) {
            ("prompt", ClientResponse::PromptRecorded { seq: 1, duplicate: false })
        } else if let ClientRequest::SessionQueue { id, .. } = &request {
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

#[test]
fn a_file_this_connection_gave_the_hub_isnt_sent_again_with_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub_that_says(&dir.path().join("hub.sock"), 0);
    let log = log();
    let attachment = &log.attachment;
    client
        .attachment_add(&"s".into(), &attachment.id, &attachment.name, attachment.media_type, log.data.clone())
        .unwrap();

    client.send_prompt_carrying(&"s".into(), "why?", None, std::slice::from_ref(&log)).unwrap();

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

    client.send_prompt_carrying(&"s".into(), "why?", None, std::slice::from_ref(&log)).unwrap();

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

    client.queue_prompt_carrying(&"s".into(), "m".into(), "why?", std::slice::from_ref(&log)).unwrap();

    let added = format!("add {}", log.attachment.id);
    let said = [added.clone(), "queued, not taken".to_string(), added, "queued".to_string()];
    assert_eq!(heard.try_iter().collect::<Vec<_>>(), said);
}
