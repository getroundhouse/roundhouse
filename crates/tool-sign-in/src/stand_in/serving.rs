use std::convert::Infallible;
use std::sync::Arc;

use http::Request;
use http::header::AUTHORIZATION;
use http_body_util::BodyExt as _;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use url::Url;

use super::authorization::visited;
use super::replies::{Reply, said, status};
use super::script::{Asks, Script, ServerMetadataAt, Wants};
use super::tokens::Tokens;
use super::{Asked, metadata};
use crate::{AuthorizationPage, ServerUrl};

/// The stand-in, serving until the runtime it was started on ends.
pub struct StandIn {
    state: Arc<State>,
}

struct State {
    at: Addresses,
    script: Mutex<Script>,
    tokens: Mutex<Tokens>,
    asked: Mutex<Vec<Asked>>,
}

/// Where the stand-in is.
pub(super) struct Addresses {
    /// Such as `http://127.0.0.1:51004`.
    pub(super) host: String,
}

impl Addresses {
    /// Its MCP server.
    fn server(&self) -> String {
        format!("{}/mcp", self.host)
    }

    /// What its MCP server's metadata says it's of.
    pub(super) fn resource(&self, script: &Script) -> String {
        if script.server_metadata_at == ServerMetadataAt::ItsHost { self.host.clone() } else { self.server() }
    }

    /// Its authorization server's name.
    pub(super) fn issuer(&self, script: &Script) -> String {
        format!("{}{}", self.host, script.issuer_path)
    }
}

impl StandIn {
    /// Serves on a port of its own, answering as `script` says.
    pub async fn with(script: Script) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = Addresses { host: format!("http://{}", listener.local_addr().unwrap()) };
        let (tokens, asked) = (Mutex::default(), Mutex::default());
        let state = Arc::new(State { at, script: Mutex::new(script), tokens, asked });
        let serving = state.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let state = serving.clone();
                let service = service_fn(move |request| {
                    let state = state.clone();
                    async move { Ok::<_, Infallible>(state.answer(request).await) }
                });
                // One that breaks off is the client's to notice.
                tokio::spawn(http1::Builder::new().serve_connection(TokioIo::new(stream), service));
            }
        });
        Self { state }
    }

    /// Its MCP server's address.
    pub fn server(&self) -> ServerUrl {
        ServerUrl::parse(&self.state.at.server()).unwrap()
    }

    /// Its authorization server's name.
    pub fn issuer(&self) -> String {
        self.state.at.issuer(&self.state.script.lock())
    }

    /// Answers otherwise from now on.
    pub fn change(&self, change: impl FnOnce(&mut Script)) {
        change(&mut self.state.script.lock());
    }

    /// Takes no access token it has given so far, as once they've ended.
    pub fn end_access_tokens(&self) {
        self.state.tokens.lock().end_access();
    }

    /// Opens `page`, as the person's browser does, and has the person do there what the script says. Gives the query of
    /// the address they're sent back to, as the app listening there gets it.
    pub fn visit(&self, page: &AuthorizationPage) -> String {
        let page = Url::parse(page.as_str()).unwrap();
        assert!(page.as_str().starts_with(&self.state.at.host), "the page is its own");
        visited(&page, &self.state.script.lock(), &self.state.at, &mut self.state.tokens.lock())
    }

    /// Each request it was sent, in the order they came.
    pub fn asked(&self) -> Vec<Asked> {
        self.state.asked.lock().clone()
    }

    /// Each it was sent whose method and path is `line`, such as `POST /token`.
    pub fn asked_for(&self, line: &str) -> Vec<Asked> {
        self.asked().into_iter().filter(|asked| asked.line == line).collect()
    }
}

impl State {
    async fn answer(&self, request: Request<Incoming>) -> Reply {
        let (head, body) = request.into_parts();
        let body = String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap();
        let path = head.uri.path().to_string();
        let asked = Asked { line: format!("{} {path}", head.method), headers: head.headers, body };
        self.asked.lock().push(asked.clone());
        let script = self.script.lock().clone();
        match asked.line.as_str() {
            "POST /mcp" => self.mcp(&asked, &script),
            "POST /token" => {
                let reply = self.tokens.lock().answer(&asked, &script);
                if let Some(gate) = &script.answers_after {
                    gate.opened().await;
                }
                reply
            }
            _ => metadata::at(&path, &script, &self.at).await.unwrap_or_else(|| status(404)),
        }
    }

    /// What its MCP server, which has two tools, answers anyone it doesn't ask for a sign-in, and whoever has one.
    fn mcp(&self, asked: &Asked, script: &Script) -> Reply {
        let message: Value = serde_json::from_str(&asked.body).unwrap();
        if script.refuses_method.is_some_and(|refused| message["method"] == refused) {
            return status(400);
        }
        if let Asks::Answers(answered) = script.asks {
            return status(answered);
        }
        let token = asked.headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()?.strip_prefix("Bearer "));
        let allowed = token.and_then(|token| self.tokens.lock().allowed(token));
        if script.asks != Asks::Nothing && allowed.is_none() {
            return metadata::unauthorized(script, &self.at);
        }
        let tool = message["params"]["name"].as_str();
        let lacks = |wants: &&Wants| {
            tool == Some(wants.tool) && !allowed.as_ref().is_some_and(|allowed| allowed.has(wants.scope))
        };
        if let Some(Wants { scope, .. }) = script.wants.iter().find(lacks) {
            return metadata::forbidden(scope);
        }
        let result = match message["method"].as_str() {
            Some("tools/list") => json!({"tools": [{"name": "find_issue"}, {"name": "close_issue"}]}),
            Some("tools/call") if tool == Some("close_issue") => {
                json!({"content": [{"type": "text", "text": "Closed issue 7"}]})
            }
            Some("tools/call") => json!({"content": [{"type": "text", "text": "Found issue 7"}]}),
            _ => json!({}),
        };
        said(200, &json!({"jsonrpc": "2.0", "id": message["id"], "result": result}))
    }
}
