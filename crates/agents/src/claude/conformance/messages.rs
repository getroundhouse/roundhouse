//! The scripted model: a stand-in for Anthropic's Messages API, whose every reply a scenario's script makes, so a
//! trial on it costs nothing and goes the same way every time. It streams as the API does, answers a count of tokens,
//! and keeps each request, which holds no credentials, since those come in headers.

use crate::conformance::http::{self, Body, Server, json_response};
use hyper::StatusCode;
use serde_json::{Value, json};

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
pub struct ScriptedModel(Server);

impl ScriptedModel {
    pub async fn start(script: Script) -> std::io::Result<Self> {
        Server::start(move |path, body, made| {
            if path.ends_with("/v1/messages/count_tokens") {
                return json_response(StatusCode::OK, &json!({"input_tokens":100}));
            }
            if !path.ends_with("/v1/messages") || !body.is_object() {
                return json_response(StatusCode::NOT_FOUND, &json!({"error":"not scripted"}));
            }
            stream(made, body, &script(&Request(body)))
        })
        .await
        .map(Self)
    }
    pub fn url(&self) -> &str {
        self.0.url()
    }
    pub fn port(&self) -> &str {
        self.0.port()
    }
    pub fn requests(&self) -> Vec<Value> {
        self.0.requests()
    }
}

/// The reply as the API streams it: the message's start, each block's start, deltas and stop, then why it stopped.
fn stream(made: u64, request: &Value, reply: &Reply) -> hyper::Response<Body> {
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

    http::events(events, made)
}
