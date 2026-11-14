//! `advise`, `advise:TEXT`, `advise-failed:CODE` and `advise-unanswered`: asking the advisor, the way the real CLI
//! sends it. The API runs the advisor, so the call is a `server_tool_use`, and its result comes in a second `assistant`
//! line of the same message.

use serde_json::json;

use crate::turn::{Turn, message_id};

/// Made up. A real one is an opaque string only the API can read.
const ENCRYPTED: &str = "bm90IHJlYWwgYWR2aWNl";

/// What the advisor sends back.
pub(crate) enum Reply<'a> {
    /// Encrypted, for the model alone.
    Encrypted,
    /// Advice that can be read.
    Said(&'a str),
    /// Why asking failed, such as `overloaded`.
    Failed(&'a str),
    /// Nothing, as for the real CLI's call made after a call to one of the agent's own tools, in the same message.
    /// Here the call has a message to itself, which is all that differs. Whatever the script does next shows the
    /// agent went on without it.
    Nothing,
}

impl<'a> Reply<'a> {
    /// The reply `command` asks for, if it's one of these.
    pub(crate) fn read(command: &'a str) -> Option<Self> {
        if command == "advise" {
            return Some(Reply::Encrypted);
        }
        if command == "advise-unanswered" {
            return Some(Reply::Nothing);
        }
        if let Some(text) = command.strip_prefix("advise:") {
            return Some(Reply::Said(text));
        }
        command.strip_prefix("advise-failed:").map(Reply::Failed)
    }
}

pub(crate) fn ask(turn: &Turn, reply: &Reply) {
    let id = message_id();
    let tool_use_id = format!("srvtoolu_{id}");
    turn.finish_message(&id, json!({"type":"server_tool_use","id":tool_use_id,"name":"advisor","input":{}}));
    let content = match reply {
        Reply::Encrypted => json!({"type":"advisor_redacted_result","encrypted_content":ENCRYPTED}),
        Reply::Said(text) => json!({"type":"advisor_result","text":text}),
        Reply::Failed(code) => json!({"type":"advisor_tool_result_error","error_code":code}),
        Reply::Nothing => return,
    };
    turn.finish_message(&id, json!({"type":"advisor_tool_result","tool_use_id":tool_use_id,"content":content}));
}
