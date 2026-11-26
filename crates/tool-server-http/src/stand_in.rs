//! A stand-in for an MCP server over HTTP, for this crate's tests, the worker's and the hub's.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::convert::Infallible;
use std::error::Error;
use std::net::Ipv4Addr;
use std::sync::Arc;

use http::header::HeaderMap;
use http::{Method, Request, Response};
use http_body_util::BodyExt as _;
use hyper::body::{Body, Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use protocol::model::{ToolServerHeader, ToolServerUrl};
use protocol::worker::{ServerHeader, ServerHeaderValue};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

/// The value of the header a stand-in's key is sent in, which nothing but the stand-in may be given.
pub const KEY: &str = "Bearer made-up-key";

/// A request as it reached a stand-in.
#[derive(Clone, Debug)]
pub struct Heard {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    /// The JSON-RPC message its body is, or `null` where it's none.
    pub message: Value,
}

impl Heard {
    /// What it asks: its message's method, or its own where it has none.
    pub fn asks(&self) -> &str {
        self.message["method"].as_str().unwrap_or(self.method.as_str())
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }
}

/// An MCP server over HTTP on this machine, which answers each request as it's told to, and goes when it's dropped.
pub struct StandIn {
    pub port: u16,
    heard: Arc<Mutex<Vec<Heard>>>,
    _serving: JoinSet<()>,
}

impl StandIn {
    pub async fn answering<B>(answer: impl Fn(&Heard) -> Response<B> + Send + Sync + 'static) -> Self
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn Error + Send + Sync>>,
    {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (heard, answer) = (Arc::new(Mutex::new(Vec::new())), Arc::new(answer));
        let hearing = heard.clone();
        let mut serving = JoinSet::new();
        serving.spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let (heard, answer) = (hearing.clone(), answer.clone());
                let service = service_fn(move |request: Request<Incoming>| {
                    let (heard, answer) = (heard.clone(), answer.clone());
                    async move {
                        let (parts, body) = request.into_parts();
                        let path = parts.uri.path_and_query().unwrap().to_string();
                        let body = body.collect().await.unwrap().to_bytes();
                        let message = serde_json::from_slice(&body).unwrap_or_default();
                        let asked = Heard { method: parts.method, path, headers: parts.headers, body, message };
                        let reply = answer(&asked);
                        heard.lock().push(asked);
                        Ok::<_, Infallible>(reply)
                    }
                });
                // One that breaks off is the client's to notice.
                connections.spawn(http1::Builder::new().serve_connection(TokioIo::new(stream), service));
            }
        });
        Self { port, heard, _serving: serving }
    }

    pub fn heard(&self) -> Vec<Heard> {
        self.heard.lock().clone()
    }

    /// What it was asked, in order, as [`Heard::asks`] says.
    pub fn asked(&self) -> Vec<String> {
        self.heard().iter().map(|heard| heard.asks().to_string()).collect()
    }

    /// Where it is, with a query that's no one's to see but the server's.
    pub fn url(&self) -> ToolServerUrl {
        ToolServerUrl::typed(&format!("http://127.0.0.1:{}/mcp?query=made-up-query", self.port)).unwrap()
    }

    /// The header that has [`KEY`].
    pub fn key() -> ServerHeader {
        ServerHeader { name: ToolServerHeader::typed("Authorization").unwrap(), value: ServerHeaderValue::new(KEY) }
    }
}
