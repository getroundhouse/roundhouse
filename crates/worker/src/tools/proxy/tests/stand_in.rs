use std::convert::Infallible;
use std::net::Ipv4Addr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http_body_util::{BodyExt, Either, Full};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::header::HeaderMap;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use protocol::model::{ToolServerHeader, ToolServerName, ToolServerUrl};
use protocol::worker::{ProxiedServer, ServerHeader, ServerHeaderValue};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// The value of the header the worker sends a stand-in, which no agent may be given.
pub(super) const KEY: &str = "Bearer made-up-key";

/// A request as it reached a stand-in.
#[derive(Clone, Debug)]
pub(super) struct Heard {
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) headers: HeaderMap,
    pub(super) body: Bytes,
}

/// A body sent as it's handed over.
pub(super) struct Events(pub(super) mpsc::Receiver<Bytes>);

impl Body for Events {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        self.0.poll_recv(cx).map(|sent| sent.map(|bytes| Ok(Frame::data(bytes))))
    }
}

pub(super) type Reply = Response<Either<Full<Bytes>, Events>>;

/// An MCP server over HTTP, which answers each request as it's told to, and goes when it's dropped.
pub(super) struct StandIn {
    pub(super) port: u16,
    heard: Arc<Mutex<Vec<Heard>>>,
    _serving: JoinSet<()>,
}

impl StandIn {
    pub(super) async fn answering(answer: impl Fn(&Heard) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let mut serving = JoinSet::new();
        serving.spawn(serve(listener, heard.clone(), Arc::new(answer)));
        Self { port, heard, _serving: serving }
    }

    pub(super) fn heard(&self) -> Vec<Heard> {
        self.heard.lock().clone()
    }

    /// The server as the hub sets it, with a query no agent may be given either, and the key.
    pub(super) fn as_set(&self) -> ProxiedServer {
        ProxiedServer {
            id: "t-1".into(),
            name: "linear_northwind".into(),
            label: ToolServerName::typed("Linear (Northwind)").unwrap(),
            url: url(&format!("http://127.0.0.1:{}/mcp?query=made-up-query", self.port)),
            header: Some(ServerHeader { name: header("Authorization"), value: ServerHeaderValue::new(KEY) }),
        }
    }
}

async fn serve(
    listener: TcpListener,
    heard: Arc<Mutex<Vec<Heard>>>,
    answer: Arc<impl Fn(&Heard) -> Reply + Send + Sync + 'static>,
) {
    let mut connections = JoinSet::new();
    while let Ok((stream, _)) = listener.accept().await {
        let (heard, answer) = (heard.clone(), answer.clone());
        connections.spawn(async move {
            let service = service_fn(move |request: Request<Incoming>| {
                let (heard, answer) = (heard.clone(), answer.clone());
                async move {
                    let (parts, body) = request.into_parts();
                    let path = parts.uri.path_and_query().unwrap().to_string();
                    let body = body.collect().await.unwrap().to_bytes();
                    let asked = Heard { method: parts.method, path, headers: parts.headers, body };
                    let reply = answer(&asked);
                    heard.lock().push(asked);
                    Ok::<_, Infallible>(reply)
                }
            });
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
        });
    }
}

pub(super) fn url(text: &str) -> ToolServerUrl {
    ToolServerUrl::typed(text).unwrap()
}

pub(super) fn header(text: &str) -> ToolServerHeader {
    ToolServerHeader::typed(text).unwrap()
}
