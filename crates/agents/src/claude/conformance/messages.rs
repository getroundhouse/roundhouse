//! The scripted model: a stand-in for Anthropic's Messages API, whose every reply a scenario's script makes, so a
//! trial on it costs nothing and goes the same way every time. It streams as the API does, answers a count of tokens,
//! and keeps each request, which holds no credentials, since those come in headers.

use std::convert::Infallible;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Channel, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::HeaderValue;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, StatusCode};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// What the scripted model says to a request, as a scenario's script decides from what the request holds.
pub type Script = fn(&Request) -> Reply;

/// A request to the scripted model, as the CLI sent it.
pub struct Request<'a>(&'a Value);

impl Request<'_> {
    /// The blocks of every message since the model last spoke, since the CLI can add a message of its own after a
    /// tool's result, such as a reminder of what git says.
    pub fn since_the_model_spoke(&self) -> Vec<Value> {
        let mut since = Vec::new();
        for message in self.0["messages"].as_array().into_iter().flatten().rev() {
            if message["role"] == "assistant" {
                break;
            }
            let content = &message["content"];
            let blocks = match content.as_str() {
                Some(text) => vec![json!({"type": "text", "text": text})],
                None => content.as_array().cloned().unwrap_or_default(),
            };
            since.splice(0..0, blocks);
        }
        since
    }

    /// The text of the tool result among [`Request::since_the_model_spoke`], or `None` where there's none.
    pub fn tool_result(&self) -> Option<String> {
        let blocks = self.since_the_model_spoke();
        let result = blocks.iter().find(|block| block["type"] == "tool_result")?;
        Some(text_of(&result["content"]))
    }

    /// What the text blocks among [`Request::since_the_model_spoke`] say, together.
    pub fn last_text(&self) -> String {
        let blocks = self.since_the_model_spoke();
        blocks.iter().filter(|block| block["type"] == "text").filter_map(|block| block["text"].as_str()).collect()
    }

    /// Whether `words` are anywhere in the request, such as in an earlier turn of the conversation.
    pub fn says(&self, words: &str) -> bool {
        self.0.to_string().contains(words)
    }

    /// What follows `words` in the request, up to the next space or quote, such as a path the CLI told the model of.
    pub fn word_after(&self, words: &str) -> Option<String> {
        let text = self.0.to_string();
        let after = &text[text.find(words)? + words.len()..];
        Some(after.split([' ', '"', '\\']).next().unwrap_or_default().to_string())
    }

    /// Whether the model called the tool `name` earlier in the conversation.
    pub fn called(&self, name: &str) -> bool {
        let messages = self.0["messages"].as_array().into_iter().flatten();
        let blocks = messages.filter(|message| message["role"] == "assistant").filter_map(|m| m["content"].as_array());
        blocks.flatten().any(|block| block["type"] == "tool_use" && block["name"] == name)
    }

    /// Whether the CLI offers the model the tool `name`.
    pub fn offers(&self, name: &str) -> bool {
        self.0["tools"].as_array().into_iter().flatten().any(|tool| tool["name"] == name)
    }
}

/// What a block's `content` says, whether it's text or a list of blocks of it.
fn text_of(content: &Value) -> String {
    match content.as_str() {
        Some(text) => text.to_string(),
        None => content.as_array().into_iter().flatten().filter_map(|part| part["text"].as_str()).collect(),
    }
}

/// What the scripted model says. It stops to have its tool called when it calls one, and ends its turn otherwise.
pub struct Reply(pub Vec<Block>);

pub enum Block {
    Text(String),
    /// What the model thought, which the API signs.
    Thinking(String),
    /// A call of the CLI's tool `name`.
    Call {
        name: String,
        input: Value,
    },
}

impl Reply {
    pub fn text(words: impl Into<String>) -> Reply {
        Reply(vec![Block::Text(words.into())])
    }

    pub fn call(name: &str, input: Value) -> Reply {
        Reply(vec![Block::Call { name: name.to_string(), input }])
    }

    /// Why the API says the reply stopped.
    fn stop(&self) -> &'static str {
        if self.0.iter().any(|block| matches!(block, Block::Call { .. })) { "tool_use" } else { "end_turn" }
    }
}

/// The scripted model, serving on 127.0.0.1 until it's dropped.
pub struct ScriptedModel {
    url: String,
    state: Arc<State>,
    server: JoinHandle<()>,
}

struct State {
    script: Script,
    requests: Mutex<Vec<Value>>,
    /// Numbers the IDs it makes, so they're the same every trial.
    made: AtomicU64,
}

impl ScriptedModel {
    pub async fn start(script: Script) -> std::io::Result<ScriptedModel> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let state = Arc::new(State { script, requests: Mutex::new(Vec::new()), made: AtomicU64::new(0) });
        let serving = state.clone();
        let server = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = serving.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |request| answer(state.clone(), request));
                    // A connection the CLI drops ends here, as one to the API would.
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                });
            }
        });
        Ok(ScriptedModel { url, state, server })
    }

    /// Where the CLI is pointed, as `ANTHROPIC_BASE_URL`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The port it listens on, which is all the sandbox lets the CLI reach.
    pub fn port(&self) -> &str {
        self.url.rsplit(':').next().unwrap_or_default()
    }

    /// Every request so far, in order, as its method, path and body.
    pub fn requests(&self) -> Vec<Value> {
        self.state.requests.lock().clone()
    }
}

impl Drop for ScriptedModel {
    fn drop(&mut self) {
        self.server.abort();
    }
}

type Body = BoxBody<Bytes, Infallible>;

async fn answer(state: Arc<State>, request: hyper::Request<Incoming>) -> Result<hyper::Response<Body>, Infallible> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let body = body.collect().await.map_or_default(http_body_util::Collected::to_bytes);
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    state.requests.lock().push(json!({"method": parts.method.as_str(), "path": path, "body": body}));
    if parts.method != Method::POST {
        return Ok(error(StatusCode::NOT_FOUND, "not_found_error", "not scripted"));
    }
    if path.ends_with("/v1/messages/count_tokens") {
        return Ok(json_response(StatusCode::OK, &json!({"input_tokens": 100})));
    }
    if !path.ends_with("/v1/messages") || !body.is_object() {
        return Ok(error(StatusCode::NOT_FOUND, "not_found_error", "not scripted"));
    }
    // A script that can't answer is the scenario's mistake, which the CLI hears as the API failing.
    let Ok(reply) = std::panic::catch_unwind(AssertUnwindSafe(|| (state.script)(&Request(&body)))) else {
        return Ok(error(StatusCode::INTERNAL_SERVER_ERROR, "api_error", "the scenario's script panicked"));
    };
    Ok(stream(&state, &body, &reply))
}

fn json_response(status: StatusCode, value: &Value) -> hyper::Response<Body> {
    let mut response = hyper::Response::new(Full::new(Bytes::from(value.to_string())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert("content-type", HeaderValue::from_static("application/json"));
    response
}

fn error(status: StatusCode, kind: &str, message: &str) -> hyper::Response<Body> {
    json_response(status, &json!({"type": "error", "error": {"type": kind, "message": message}}))
}

/// The reply as the API streams it: the message's start, each block's start, deltas and stop, then why it stopped.
fn stream(state: &State, request: &Value, reply: &Reply) -> hyper::Response<Body> {
    let made = state.made.fetch_add(1, Ordering::Relaxed);
    let model = request["model"].as_str().unwrap_or("scripted");
    let usage = json!({"input_tokens": 100, "output_tokens": 1, "cache_creation_input_tokens": 0,
                       "cache_read_input_tokens": 0});
    let message = json!({"id": format!("msg_scripted_{made}"), "type": "message", "role": "assistant", "model": model,
                         "content": [], "stop_reason": null, "stop_sequence": null, "usage": usage});
    let mut events = vec![("message_start", json!({"type": "message_start", "message": message}))];
    for (index, block) in reply.0.iter().enumerate() {
        let (start, deltas) = match block {
            Block::Text(text) => {
                (json!({"type": "text", "text": ""}), vec![json!({"type": "text_delta", "text": text})])
            }
            Block::Thinking(thinking) => (
                json!({"type": "thinking", "thinking": "", "signature": ""}),
                vec![
                    json!({"type": "thinking_delta", "thinking": thinking}),
                    json!({"type": "signature_delta", "signature": "c2NyaXB0ZWQ="}),
                ],
            ),
            Block::Call { name, input } => (
                json!({"type": "tool_use", "id": format!("toolu_scripted_{made}_{index}"), "name": name, "input": {}}),
                vec![json!({"type": "input_json_delta", "partial_json": input.to_string()})],
            ),
        };
        let start = json!({"type": "content_block_start", "index": index, "content_block": start});
        events.push(("content_block_start", start));
        for delta in deltas {
            events
                .push(("content_block_delta", json!({"type": "content_block_delta", "index": index, "delta": delta})));
        }
        events.push(("content_block_stop", json!({"type": "content_block_stop", "index": index})));
    }
    let stopped = json!({"type": "message_delta", "delta": {"stop_reason": reply.stop(), "stop_sequence": null},
                         "usage": {"output_tokens": 20}});
    events.push(("message_delta", stopped));
    events.push(("message_stop", json!({"type": "message_stop"})));

    let (mut sender, body) = Channel::<Bytes, Infallible>::new(4);
    tokio::spawn(async move {
        for (name, data) in events {
            let event = Bytes::from(format!("event: {name}\ndata: {data}\n\n"));
            // The CLI gave up on the request, as a cancel does.
            if sender.send_data(event).await.is_err() {
                return;
            }
        }
    });
    let mut response = hyper::Response::new(body.boxed());
    let headers = response.headers_mut();
    headers.insert("content-type", HeaderValue::from_static("text/event-stream"));
    if let Ok(id) = HeaderValue::from_str(&format!("req_scripted_{made}")) {
        headers.insert("request-id", id);
    }
    response
}
