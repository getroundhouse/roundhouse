use std::net::Ipv4Addr;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HOST, HeaderMap, HeaderName, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use protocol::Build;
use protocol::worker::{ProxiedServer, ToolsToken};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::stand_in::KEY;
use crate::asks::Asks;
use crate::tools::{Caller, Tools};

/// An agent's end of a worker that has tool servers set, over HTTP, as its CLI reaches it.
pub(super) struct Agent {
    pub(super) tools: Arc<Tools>,
    port: u16,
}

impl Agent {
    pub(super) async fn with(servers: Vec<ProxiedServer>) -> Self {
        let (frames, _) = mpsc::channel(8);
        let tools = Tools::new(Arc::new(Asks::new(frames)), Build::UNSTAMPED, crate::Timers::DEFAULT.wait_poll);
        let caller = Caller { session_id: "s".into(), worktree: "/no/worktree".into() };
        tools.admit(ToolsToken::new("token-s"), caller);
        tools.proxy.set(servers);
        let tools = Arc::new(tools);
        let port = tools.serve(None).await.unwrap();
        Self { tools, port }
    }

    /// The path of the only server, as an agent is launched with it.
    pub(super) async fn path(&self) -> String {
        let addresses = self.tools.addresses().await.unwrap();
        let [(_, url)] = &addresses.servers[..] else { panic!("{addresses:?}") };
        url.strip_prefix(&format!("http://127.0.0.1:{}", self.port)).unwrap().to_string()
    }

    /// Sends a request with the session's token, and `headers` over the rest, each of which an empty value takes off.
    pub(super) async fn request(
        &self,
        method: Method,
        path: &str,
        headers: &[(&'static str, &str)],
        body: impl Into<Bytes>,
    ) -> Response<Incoming> {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).await.unwrap();
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
        tokio::spawn(connection);
        let mut request = Request::new(Full::new(body.into()));
        *request.method_mut() = method;
        *request.uri_mut() = path.parse().unwrap();
        let host = format!("127.0.0.1:{}", self.port);
        let sent = [(HOST.as_str(), host.as_str()), ("authorization", "Bearer token-s")];
        for (name, value) in sent.into_iter().chain(headers.iter().copied()) {
            let name = HeaderName::from_bytes(name.as_bytes()).unwrap();
            if value.is_empty() {
                request.headers_mut().remove(name);
            } else {
                request.headers_mut().insert(name, HeaderValue::from_str(value).unwrap());
            }
        }
        sender.send_request(request).await.unwrap()
    }

    /// The connection a message was sent on, with no answer read, which ends as it's dropped.
    pub(super) async fn raw(&self, path: &str, body: &str) -> TcpStream {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).await.unwrap();
        let headers = format!("Host: 127.0.0.1:{}\r\nAuthorization: Bearer token-s", self.port);
        let request = format!("POST {path} HTTP/1.1\r\n{headers}\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        stream.write_all(request.as_bytes()).await.unwrap();
        stream
    }

    /// Sends a message to the only server, and reads all of what came back.
    pub(super) async fn post(&self, body: impl Into<Bytes>) -> Answer {
        Answer::of(self.request(Method::POST, &self.path().await, &[], body).await).await
    }

    pub(super) async fn get(&self) -> Answer {
        Answer::of(self.request(Method::GET, &self.path().await, &[], "").await).await
    }
}

/// All an agent gets back, none of which may have the key, or what the server's URL has in its query.
#[derive(Debug)]
pub(super) struct Answer {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) body: String,
}

impl Answer {
    pub(super) async fn of(response: Response<Incoming>) -> Self {
        let (parts, body) = response.into_parts();
        let body = String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap();
        let answer = Self { status: parts.status, headers: parts.headers, body };
        let all = format!("{answer:?}");
        assert!(!all.contains(KEY) && !all.contains("made-up"), "{all}");
        answer
    }

    pub(super) fn header(&self, name: &str) -> &str {
        self.headers[name].to_str().unwrap()
    }

    /// The ID and the message of the JSON-RPC error it is.
    pub(super) fn error(&self) -> (Value, String) {
        assert_eq!((self.status, self.header("content-type")), (StatusCode::OK, "application/json"), "{self:?}");
        let body: Value = serde_json::from_str(&self.body).unwrap();
        assert_eq!((&body["jsonrpc"], &body["error"]["code"]), (&"2.0".into(), &(-32603).into()), "{body}");
        (body["id"].clone(), body["error"]["message"].as_str().unwrap().to_string())
    }
}
