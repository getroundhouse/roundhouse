//! A hub stood in for on a Unix socket, for a test of what talks to one, here or in a client's. It answers each hello,
//! wherever it comes, as a hub of this build would unless the test says otherwise, and does with each other request
//! what the test says, in the order they come, which can be to answer and then send what a subscription sends.
#![cfg(unix)]
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HelloReply, HubFrame, ReplyOutcome};
use protocol::frame;

use crate::Client;

/// What the stand-in does with a request.
pub enum Does {
    Answers(Box<ReplyOutcome>),
    /// Answers, then sends these, as a hub follows what a client subscribed to.
    AnswersThen(Box<ReplyOutcome>, Vec<HubFrame>),
    /// Does what the second says once the first is sent something, on a thread of its own, so the requests that come
    /// meanwhile are done meanwhile, as a hub answers a request that takes a while.
    After(flume::Receiver<()>, Box<Does>),
    /// Writes this line as it is, such as a reply no build of this client can read.
    Writes(String),
    /// Nothing, as with a request the test isn't about.
    Nothing,
    /// Goes away, as a hub that's stopped does.
    HangsUp,
}

/// A stand-in that says it's a hub of build "a", to the first client to connect, unless it's told otherwise.
pub struct StandIn {
    hello: ReplyOutcome,
    every_connection: bool,
}

impl Default for StandIn {
    fn default() -> Self {
        Self::of_build("a")
    }
}

impl StandIn {
    /// One that says it's a hub of `build` that speaks this build's protocol.
    pub fn of_build(build: &str) -> Self {
        let hello = HelloReply { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: build.into() };
        Self::greeting(ClientResponse::Hello(hello))
    }

    /// One that answers each hello with `hello`, such as one that speaks a protocol this build doesn't.
    pub fn greeting(hello: ClientResponse) -> Self {
        Self { hello: ReplyOutcome::Response(hello), every_connection: false }
    }

    /// Serves each client that connects, rather than only the first, as a hub a client connects to again does. What
    /// it's told to do with a request is done for one connection at a time.
    #[must_use]
    pub fn to_every_connection(self) -> Self {
        Self { every_connection: true, ..self }
    }

    /// Stands in on `socket`, doing with each request but a hello what `does` says. It says each request it's sent,
    /// hellos too, as it comes, to the receiver it returns, which nothing need read.
    pub fn listen(
        self,
        socket: &Path,
        does: impl FnMut(ClientRequest) -> Does + Send + 'static,
    ) -> flume::Receiver<ClientRequest> {
        let listener = UnixListener::bind(socket).unwrap();
        let (asked, heard) = flume::unbounded();
        let hub = Arc::new(Hub { hello: self.hello, does: Mutex::new(Box::new(does)), asked });
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let hub = hub.clone();
                if !self.every_connection {
                    return hub.serve(stream.unwrap());
                }
                std::thread::spawn(move || hub.serve(stream.unwrap()));
            }
        });
        heard
    }
}

/// Stands in for a hub on `socket`, doing with each request what `does` says, and connects to it.
pub fn connect(socket: &Path, does: impl FnMut(ClientRequest) -> Does + Send + 'static) -> Client {
    listen(socket, does);
    Client::connect(socket, "test").unwrap()
}

/// Stands in for a hub on `socket`, doing with each request what `does` says, for the first client to connect, as
/// [`StandIn::listen`] says.
pub fn listen(
    socket: &Path,
    does: impl FnMut(ClientRequest) -> Does + Send + 'static,
) -> flume::Receiver<ClientRequest> {
    StandIn::default().listen(socket, does)
}

struct Hub {
    hello: ReplyOutcome,
    does: Mutex<Box<dyn FnMut(ClientRequest) -> Does + Send>>,
    asked: flume::Sender<ClientRequest>,
}

impl Hub {
    fn serve(&self, stream: UnixStream) {
        let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
        for line in BufReader::new(stream).lines().map_while(std::result::Result::ok) {
            let ClientFrame { id, request } = frame::decode(line.as_bytes()).unwrap();
            // Nobody may be listening, for a stand-in whose requests a test doesn't look at.
            let _ = self.asked.send(request.clone());
            let does = if matches!(request, ClientRequest::Hello(_)) {
                Does::Answers(Box::new(self.hello.clone()))
            } else {
                (self.does.lock())(request)
            };
            if !does_it(&writer, id, does) {
                // At once, though a request waiting with `Does::After` still holds the connection.
                let _ = writer.lock().shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
}

/// Does what's said with request `id`, writing to the client on `writer`. Returns false once the client has gone, or
/// the stand-in has hung up on it.
fn does_it(writer: &Arc<Mutex<UnixStream>>, id: u64, does: Does) -> bool {
    let sent = |frames: &[HubFrame]| {
        // One write at a time, so what's sent after a reply follows it, though another thread writes too.
        let mut writer = writer.lock();
        frames.iter().all(|frame| writer.write_all(&frame::encode(frame).unwrap()).is_ok())
    };
    match does {
        Does::Answers(outcome) => sent(&[HubFrame::Reply { id, outcome: *outcome }]),
        Does::AnswersThen(outcome, then) => {
            let mut frames = vec![HubFrame::Reply { id, outcome: *outcome }];
            frames.extend(then);
            sent(&frames)
        }
        Does::After(until, then) => {
            let writer = writer.clone();
            std::thread::spawn(move || {
                if until.recv().is_ok() && !does_it(&writer, id, *then) {
                    let _ = writer.lock().shutdown(std::net::Shutdown::Both);
                }
            });
            true
        }
        Does::Writes(line) => writer.lock().write_all(format!("{line}\n").as_bytes()).is_ok(),
        Does::Nothing => true,
        Does::HangsUp => false,
    }
}
