//! A connection that sends frames as written in a test, for what `Connection` has no way to send, and keeps what the
//! hub sends on it.

use std::io::{BufRead as _, BufReader, ErrorKind, Write as _};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::{ClientRequest, ClientResponse, HUB_SPEAKS, Hello, HubFrame, HubNotice, ReplyOutcome};
use protocol::model::{ChangeSummary, Observation, Observed};

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

    /// Reads into `sent` what the hub sends within `bound`, for a test of what it doesn't send, and gives back what
    /// came.
    pub(crate) fn within(&mut self, bound: Duration) -> &[HubFrame] {
        let before = self.sent.len();
        self.socket.set_read_timeout(Some(bound)).unwrap();
        loop {
            let mut line = String::new();
            match self.replies.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => self.sent.push(protocol::frame::decode(line.as_bytes()).unwrap()),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(e) => panic!("reading what the hub sent: {e}"),
            }
        }
        self.socket.set_read_timeout(Some(TIMEOUT)).unwrap();
        &self.sent[before..]
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

/// A connection that has said hello at the newest version this hub speaks, and follows each of `follows` through one
/// `Subscribe`, and sends nothing after.
pub(crate) fn following(stack: &Stack, follows: Vec<Follow>) -> RawConnection {
    let mut connection = RawConnection::open(stack);
    let hello = Hello {
        protocol_version: HUB_SPEAKS.newest,
        client: "following".into(),
        oldest_protocol_version: None,
        device: None,
    };
    let greeted = connection.call(0, &ClientRequest::Hello(hello));
    assert!(matches!(greeted, ReplyOutcome::Response(ClientResponse::Hello(_))), "{greeted:?}");
    let start =
        (1..).zip(follows).map(|(subscription_id, follow)| Subscription::new(subscription_id, follow)).collect();
    let subscribed = connection.call(1, &ClientRequest::Subscribe(Subscribe { stop: Vec::new(), start }));
    assert!(matches!(subscribed, ReplyOutcome::Response(ClientResponse::Subscribed(_))), "{subscribed:?}");
    connection
}

/// The changed files `frame` tells of, if it tells of any.
pub(crate) fn changes_in(frame: &HubFrame) -> Option<&Observed<ChangeSummary>> {
    let HubFrame::Notice { notice: HubNotice::Observation(Observation::WorkspaceChanges { changes, .. }), .. } = frame
    else {
        return None;
    };
    Some(changes)
}
