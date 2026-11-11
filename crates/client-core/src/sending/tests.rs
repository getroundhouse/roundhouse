// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;

use protocol::PROTOCOL_VERSION;
use protocol::attachment::MediaType;
use protocol::client::{ClientResponse, HubFrame, ReplyOutcome};
use protocol::frame;

use super::*;

/// Answers a hello, then hangs up, so every call after it fails.
fn connected_to_a_hub_that_goes_away(socket: &Path) -> Client {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        BufReader::new(stream.try_clone().unwrap()).lines().next();
        let hello = ClientResponse::Hello { protocol_version: PROTOCOL_VERSION, hub_version: "a".to_string() };
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
