//! A connection that sends frames as written in a test, for what `Client` has no way to send, and keeps what the hub
//! sends on it.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;

use protocol::client::{ClientRequest, HubFrame, ReplyOutcome};

use super::{Stack, TIMEOUT};

pub(crate) struct RawConnection {
    socket: UnixStream,
    replies: BufReader<UnixStream>,
    /// What the hub has sent but replies, in order.
    pub(crate) sent: Vec<HubFrame>,
}

impl RawConnection {
    pub(crate) fn open(stack: &Stack) -> Self {
        let socket = UnixStream::connect(stack.socket()).unwrap();
        socket.set_read_timeout(Some(TIMEOUT)).unwrap();
        let replies = BufReader::new(socket.try_clone().unwrap());
        Self { socket, replies, sent: Vec::new() }
    }

    /// Sends `request`, a request's JSON, as call `id`, and gives back its reply.
    pub(crate) fn ask(&mut self, id: u64, request: &str) -> ReplyOutcome {
        self.ask_unless_closed(id, request)
            .unwrap_or_else(|| panic!("the hub closed the connection before replying to {id}"))
    }

    /// The same, for `request` as this build writes it.
    pub(crate) fn call(&mut self, id: u64, request: &ClientRequest) -> ReplyOutcome {
        self.ask(id, &serde_json::to_string(request).unwrap())
    }

    /// `None` if the hub closes the connection before it replies. Keeps in `sent` whatever else the hub sends first.
    pub(crate) fn ask_unless_closed(&mut self, id: u64, request: &str) -> Option<ReplyOutcome> {
        writeln!(self.socket, r#"{{"id":{id},"request":{request}}}"#).unwrap();
        loop {
            match self.next()? {
                HubFrame::Reply { id: answered, outcome } if answered == id => return Some(outcome),
                frame => self.sent.push(frame),
            }
        }
    }

    /// Reads what the hub sends into `sent` until it holds `done`, failing if the hub sends nothing for [`TIMEOUT`].
    pub(crate) fn until(&mut self, what: &str, done: impl Fn(&[HubFrame]) -> bool) {
        while !done(&self.sent) {
            let frame = self.next().unwrap_or_else(|| panic!("the hub closed the connection waiting for {what}"));
            self.sent.push(frame);
        }
    }

    /// The next frame the hub sends, or `None` once it has closed the connection.
    fn next(&mut self) -> Option<HubFrame> {
        let mut line = String::new();
        if self.replies.read_line(&mut line).unwrap() == 0 {
            return None;
        }
        Some(protocol::frame::decode(line.as_bytes()).unwrap())
    }
}
