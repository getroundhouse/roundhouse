use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::header::{CONTENT_TYPE, HeaderMap};
use http::{Method, Request, Response};
use http_body_util::{BodyExt as _, Either, Full};
use hyper::body::{Bytes, Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use protocol::client::tool_server_checks::NoToolList;
use protocol::model::{ToolServerHeader, ToolServerUrl};
use protocol::worker::{ServerHeader, ServerHeaderValue};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::ToolSignIn;

/// What the header a key is sent in holds.
pub(super) const KEY: &str = "Bearer made-up-key";

/// A request as it reached a stand-in.
#[derive(Clone, Debug)]
pub(super) struct Heard {
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) headers: HeaderMap,
    pub(super) message: Value,
}

impl Heard {
    /// What it asks: its message's method, or its own where it has none.
    pub(super) fn asks(&self) -> &str {
        self.message["method"].as_str().unwrap_or(self.method.as_str())
    }

    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }
}

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

/// An MCP server over HTTP, which answers each request as it's told to, until the test ends.
pub(super) struct StandIn {
    pub(super) port: u16,
    heard: Arc<Mutex<Vec<Heard>>>,
}

impl StandIn {
    pub(super) async fn answering(answer: impl Fn(&Heard) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (heard, answer) = (Arc::new(Mutex::new(Vec::new())), Arc::new(answer));
        let hearing = heard.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (heard, answer) = (hearing.clone(), answer.clone());
                let service = service_fn(move |request: Request<Incoming>| {
                    let (heard, answer) = (heard.clone(), answer.clone());
                    async move {
                        let (parts, body) = request.into_parts();
                        let path = parts.uri.path_and_query().unwrap().to_string();
                        let body = body.collect().await.unwrap().to_bytes();
                        let message = serde_json::from_slice(&body).unwrap_or_default();
                        let asked = Heard { method: parts.method, path, headers: parts.headers, message };
                        let reply = answer(&asked);
                        heard.lock().push(asked);
                        Ok::<_, Infallible>(reply)
                    }
                });
                // One that breaks off is the client's to notice.
                tokio::spawn(http1::Builder::new().serve_connection(TokioIo::new(stream), service));
            }
        });
        Self { port, heard }
    }

    pub(super) fn heard(&self) -> Vec<Heard> {
        self.heard.lock().clone()
    }

    /// What it was asked, in order.
    pub(super) fn asked(&self) -> Vec<String> {
        self.heard().iter().map(|heard| heard.asks().to_string()).collect()
    }

    pub(super) fn url(&self) -> ToolServerUrl {
        url(&format!("http://127.0.0.1:{}/mcp?tools=some", self.port))
    }

    /// Asks it for its tools with [`KEY`].
    pub(super) async fn with_key(&self) -> Result<u32, NoToolList> {
        list_tools(&self.url(), true).await
    }

    /// The same with none.
    pub(super) async fn with_none(&self) -> Result<u32, NoToolList> {
        list_tools(&self.url(), false).await
    }
}

pub(super) fn url(text: &str) -> ToolServerUrl {
    ToolServerUrl::typed(text).unwrap()
}

pub(super) async fn list_tools(server: &ToolServerUrl, keyed: bool) -> Result<u32, NoToolList> {
    let name = ToolServerHeader::typed("Authorization").unwrap();
    let header = keyed.then(|| ServerHeader { name, value: ServerHeaderValue::new(KEY) });
    ToolSignIn::new("https://client.example").list_tools(server, header.as_ref()).await
}
