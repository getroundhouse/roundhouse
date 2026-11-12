//! A hub stood in for on a Unix socket, for a test of what talks to one. It says hello, then does with each request
//! what the test says.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use protocol::frame;

use crate::Client;

/// What the stand-in does with a request.
pub(crate) enum Does {
    Answers(Box<ReplyOutcome>),
    /// Nothing, as with a request the test isn't about.
    Nothing,
    /// Goes away, as a hub that's stopped does.
    HangsUp,
}

/// Stands in for a hub on `socket`, doing with each request what `does` says, and connects to it.
pub(crate) fn connect(socket: &Path, mut does: impl FnMut(ClientRequest) -> Does + Send + 'static) -> Client {
    let listener = UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
        lines.next();
        let hello = ClientResponse::Hello { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: "a".to_string() };
        reply(&mut stream, 1, ReplyOutcome::Response(hello));
        for line in lines.map_while(std::result::Result::ok) {
            let frame: ClientFrame = serde_json::from_str(&line).unwrap();
            match does(frame.request) {
                Does::Answers(outcome) => reply(&mut stream, frame.id, *outcome),
                Does::Nothing => {}
                Does::HangsUp => return,
            }
        }
    });
    Client::connect(socket, "test").unwrap()
}

fn reply(stream: &mut UnixStream, id: u64, outcome: ReplyOutcome) {
    stream.write_all(&frame::encode(&HubFrame::Reply { id, outcome }).unwrap()).unwrap();
}
