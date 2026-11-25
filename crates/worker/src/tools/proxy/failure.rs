use crate::tools::http::{AnswerBody, whole};
use hyper::header::HeaderValue;
use hyper::{Response, StatusCode};
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::{Value, json};

/// JSON-RPC's code for an error inside whoever answers, which a server that can't be used is to its client.
const INTERNAL_ERROR: i32 = -32603;

/// Why a request got no answer from its server, or one that isn't passed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// The hub has the server no more.
    Removed,
    /// Its URL or its header isn't one HTTP can be sent with.
    Unusable,
    /// It answered 401 or 403 to a request with its key, or to one without.
    Refused { with_key: bool, status: StatusCode },
    /// It sent the request somewhere else.
    Redirected,
    /// There was no connecting to it.
    Unreachable,
    /// It was connected to, and went before it answered.
    Lost,
    /// Nothing says which servers this machine trusts.
    NoCertificates,
}

impl Failure {
    /// What the agent reads, where `label` is what the person calls the server. It's made of these words alone, so it
    /// holds no key, and nothing of a URL.
    fn said(self, label: &str) -> String {
        match self {
            Failure::Removed => "This tool server was removed from Roundhouse, so its tools can't be used.".into(),
            Failure::Unusable => {
                format!("Roundhouse can't use the URL or the header {label} has. Change it in Settings › MCP servers.")
            }
            Failure::Refused { with_key: true, status } => {
                let status = status.as_u16();
                format!("{label} refused the key Roundhouse sent it ({status}). Change it in Settings › MCP servers.")
            }
            Failure::Refused { with_key: false, status } => {
                let status = status.as_u16();
                format!("{label} refused a request without a key ({status}). Give it one in Settings › MCP servers.")
            }
            Failure::Redirected => format!(
                "{label} redirected the request to another address, which Roundhouse doesn't follow. Change its URL in \
                 Settings › MCP servers."
            ),
            Failure::Unreachable => format!("{label} couldn't be reached from this machine."),
            Failure::Lost => format!(
                "{label} closed the connection before it answered, so what was asked of it may or may not have been \
                 done."
            ),
            Failure::NoCertificates => {
                format!("{label} couldn't be reached, since this machine's certificates couldn't be read.")
            }
        }
    }

    /// The answer to a request whose body is `asked`: a JSON-RPC error for a request with an ID, and otherwise a status
    /// that says it's the server that failed, which isn't one a CLI takes for the session's token being rejected.
    pub(super) fn answer(self, label: &str, asked: &[u8]) -> Response<AnswerBody> {
        let said = self.said(label);
        match request_id(asked) {
            Some(id) => {
                let error = json!({ "code": INTERNAL_ERROR, "message": said });
                let body = json!({ "jsonrpc": "2.0", "id": id, "error": error }).to_string();
                whole(StatusCode::OK, HeaderValue::from_static("application/json"), body)
            }
            None => whole(StatusCode::BAD_GATEWAY, HeaderValue::from_static("text/plain; charset=utf-8"), said),
        }
    }
}

/// A JSON-RPC request, which has both. A notification has no ID, and an answer to the server's own request no method.
#[derive(Deserialize)]
struct Message {
    #[serde(rename = "method")]
    _method: IgnoredAny,
    id: Value,
}

fn request_id(body: &[u8]) -> Option<Value> {
    let Message { id, .. } = serde_json::from_slice(body).ok()?;
    (!id.is_null()).then_some(id)
}
