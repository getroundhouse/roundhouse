//! A connection to a hub, served as one on its socket is, over a stream in memory, for a test to send requests on and
//! read what the hub sends. Every frame it reads is checked to carry a revision on each record and observation, as
//! [`super::revised`] says.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ClientRequest, ClientResponse, Hello, HubFrame, ReplyOutcome};
use protocol::frame;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::time::timeout;

use super::super::admit::Gate;
use super::super::connection::serve_connection;
use super::revised;
use crate::hub::Hub;

/// Long enough for anything here on a busy machine.
pub(in crate::server) const WAIT: Duration = Duration::from_secs(10);

/// A connection to `hub`, served as one on its socket is.
pub(in crate::server) struct Connection {
    replies: Lines<BufReader<ReadHalf<DuplexStream>>>,
    frames: WriteHalf<DuplexStream>,
    /// What the hub has sent but replies, in order, as [`Connection::reply`] and [`Connection::until`] read it.
    pub(in crate::server) sent: Vec<HubFrame>,
}

impl Connection {
    pub(in crate::server) fn open(hub: &Arc<Hub>) -> Self {
        let (client, served) = tokio::io::duplex(1 << 16);
        let (read, write) = tokio::io::split(served);
        tokio::spawn(serve_connection(hub.clone(), read, write, Gate::local(hub)));
        let (replies, frames) = tokio::io::split(client);
        Self { replies: BufReader::new(replies).lines(), frames, sent: Vec::new() }
    }

    /// One that has said hello to `hub` at version `version` of the client protocol.
    pub(in crate::server) async fn greeted(hub: &Arc<Hub>, version: u32) -> Self {
        let mut connection = Self::open(hub);
        let hello = ClientRequest::Hello(Hello {
            protocol_version: version,
            client: "test".into(),
            oldest_protocol_version: None,
            device: None,
        });
        let greeted = connection.reply(0, &hello).await;
        assert!(matches!(greeted, ReplyOutcome::Response(ClientResponse::Hello(_))), "{greeted:?}");
        connection
    }

    /// Sends `line`, for `case`, and gives back the next frame, or `None` once the hub has closed the connection.
    pub(super) async fn send(&mut self, line: &str, case: &str) -> Option<HubFrame> {
        // A hub that has closed the connection may no longer take it, which what it sends back says.
        let _ = self.frames.write_all(format!("{line}\n").as_bytes()).await;
        let next = timeout(WAIT, self.replies.next_line()).await;
        let next =
            next.unwrap_or_else(|_| panic!("{case}: the hub neither replied nor closed the connection")).unwrap();
        next.map(|line| revised::checked(frame::decode(line.as_bytes()).unwrap()))
    }

    /// Sends `request` as call `id`, and gives back its reply, keeping in `sent` whatever else the hub sends first.
    pub(in crate::server) async fn reply(&mut self, id: u64, request: &ClientRequest) -> ReplyOutcome {
        self.write(id, request).await;
        self.reply_to(id).await
    }

    /// Sends `request` as call `id`, without waiting for its reply.
    pub(in crate::server) async fn write(&mut self, id: u64, request: &ClientRequest) {
        let line = call(id, &serde_json::to_string(request).unwrap());
        self.frames.write_all(format!("{line}\n").as_bytes()).await.unwrap();
    }

    /// The reply to call `id`, keeping in `sent` whatever else the hub sends first.
    pub(in crate::server) async fn reply_to(&mut self, id: u64) -> ReplyOutcome {
        loop {
            match self.next().await {
                Some(HubFrame::Reply { id: answered, outcome }) if answered == id => return outcome,
                Some(frame) => self.sent.push(frame),
                None => panic!("the hub closed the connection"),
            }
        }
    }

    /// Reads what the hub sends into `sent` until it holds `done`.
    pub(in crate::server) async fn until(&mut self, what: &str, done: impl Fn(&[HubFrame]) -> bool) {
        while !done(&self.sent) {
            match self.next().await {
                Some(frame) => self.sent.push(frame),
                None => panic!("the hub closed the connection waiting for {what}, having sent {:#?}", self.sent),
            }
        }
    }

    /// Reads what the hub sends into `sent` until it closes the connection.
    pub(in crate::server) async fn until_closed(&mut self) {
        while let Some(frame) = self.next().await {
            self.sent.push(frame);
        }
    }

    /// The next frame the hub sends, or `None` once it has closed the connection.
    async fn next(&mut self) -> Option<HubFrame> {
        let line = timeout(WAIT, self.replies.next_line()).await;
        let line = line.unwrap_or_else(|_| panic!("the hub sent nothing more, having sent {:#?}", self.sent)).unwrap();
        line.map(|line| revised::checked(frame::decode(line.as_bytes()).unwrap()))
    }
}

/// A call of `request`, a request's JSON, as `id`.
pub(super) fn call(id: u64, request: &str) -> String {
    format!(r#"{{"id":{id},"request":{request}}}"#)
}
