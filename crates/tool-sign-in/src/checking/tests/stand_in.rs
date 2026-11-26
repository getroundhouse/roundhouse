use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use http::Response;
use http::header::CONTENT_TYPE;
use http_body_util::{Either, Full};
use hyper::body::{Bytes, Frame};
use protocol::client::tool_server_checks::NoToolList;
use protocol::model::ToolServerUrl;
use serde_json::{Value, json};
use tool_server_http::StandIn;

use crate::ToolSignIn;

/// A body that's sent as far as this, and never ends.
pub(super) struct Held(Option<Bytes>);

impl hyper::body::Body for Held {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        self.0.take().map_or(Poll::Pending, |bytes| Poll::Ready(Some(Ok(Frame::data(bytes)))))
    }
}

pub(super) type Body = Either<Full<Bytes>, Held>;
pub(super) type Reply = Response<Body>;

/// `status`, with nothing.
pub(super) fn status(status: u16) -> Reply {
    Response::builder().status(status).body(Either::Left(Full::default())).unwrap()
}

pub(super) fn as_json(body: &Value) -> Reply {
    let reply = Response::builder().header(CONTENT_TYPE, "application/json");
    reply.body(Either::Left(Full::from(body.to_string()))).unwrap()
}

/// As an event of a stream, after others that aren't an answer, which stays open after it.
pub(super) fn as_stream(body: &Value) -> Reply {
    let progress = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {"made": "up"}});
    let events = format!(": waiting\n\nid: 1\ndata:\n\nevent: message\ndata: {progress}\n\ndata: {body}\n\n");
    held_after("text/event-stream; charset=utf-8", events)
}

pub(super) fn held_after(content_type: &str, sent: impl Into<Bytes>) -> Reply {
    Response::builder().header(CONTENT_TYPE, content_type).body(Either::Right(Held(Some(sent.into())))).unwrap()
}

/// A server's answer to a request for its tools, of which there are two.
pub(super) fn listed() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": [{"name": "find"}, {"name": "close"}], "nextCursor": "more"}})
}

/// Asks `server` for its tools with its key.
pub(super) async fn with_key(server: &StandIn) -> Result<u32, NoToolList> {
    list_tools(&server.url(), true).await
}

/// The same with none.
pub(super) async fn with_none(server: &StandIn) -> Result<u32, NoToolList> {
    list_tools(&server.url(), false).await
}

pub(super) fn url(text: &str) -> ToolServerUrl {
    ToolServerUrl::typed(text).unwrap()
}

pub(super) async fn list_tools(server: &ToolServerUrl, keyed: bool) -> Result<u32, NoToolList> {
    let key = StandIn::key();
    ToolSignIn::new("https://client.example").list_tools(server, keyed.then_some(&key)).await
}
