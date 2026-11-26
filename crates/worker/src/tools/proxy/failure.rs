use super::message::Message;
use crate::tools::http::{AnswerBody, whole};
use hyper::header::HeaderValue;
use hyper::{Response, StatusCode};
use protocol::worker::ToolAccess;
use serde_json::json;

/// JSON-RPC's code for an error inside whoever answers, which a server that can't be used is to its client.
const INTERNAL_ERROR: i32 = -32603;

/// Why a request got no answer from its server, or one that isn't passed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// The person has moved it to another URL since the agent connected to it.
    Moved,
    /// Its URL or its header isn't one HTTP can be sent with.
    Unusable,
    /// It's signed in to, and needs the person to again, so it wasn't asked.
    NeedsSignIn,
    /// It answered 401 or 403 to a request that carried `sent`.
    Refused { sent: Sent, status: StatusCode },
    /// It answered 403 to its access token, saying it wants more access, which the hub made this of.
    WantsAccess(ToolAccess),
    /// It sent the request somewhere else.
    Redirected,
    /// There was no connecting to it.
    Unreachable,
    /// It was connected to, and went before it answered.
    Lost,
    /// Nothing says which servers this machine trusts.
    NoCertificates,
}

/// What a request told a server of who's asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sent {
    Nothing,
    Key,
    AccessToken,
}

impl Failure {
    /// What the agent reads, where `label` is what the person calls the server. It's made of these words alone, so it
    /// holds no key, and nothing of a URL.
    fn said(self, label: &str) -> String {
        match self {
            Failure::Moved => format!(
                "{label} has been moved to another URL in Roundhouse, so nothing is sent to it until this agent connects \
                 to it again."
            ),
            Failure::Unusable => {
                format!("Roundhouse can't use the URL or the header {label} has. Edit it in Settings › MCP servers.")
            }
            Failure::NeedsSignIn => {
                format!("{label} needs you to sign in to it. Sign in from Settings › MCP servers.")
            }
            Failure::Refused { sent: Sent::AccessToken, status } => {
                let status = status.as_u16();
                format!(
                    "{label} refused the sign-in Roundhouse has to it ({status}). Sign in to it again from Settings › MCP \
                     servers."
                )
            }
            Failure::Refused { sent: Sent::Key, status } => {
                let status = status.as_u16();
                format!(
                    "{label} refused the key Roundhouse sent it ({status}). Give it another in Settings › MCP servers."
                )
            }
            Failure::Refused { sent: Sent::Nothing, status } => {
                let status = status.as_u16();
                format!("{label} refused a request without a key ({status}). Give it one in Settings › MCP servers.")
            }
            Failure::WantsAccess(access) => {
                let then = match access {
                    ToolAccess::SignInAgain | ToolAccess::Unknown => "Sign in to it again from Settings › MCP servers.",
                    ToolAccess::NotAskedFor => {
                        "Signing in to it again would ask for no more, so the account signed in with may not have it."
                    }
                    ToolAccess::OtherToken => "Its sign-in has changed since, so this may work if it's tried again.",
                };
                format!("{label} wants more access for this than its sign-in has (403). {then}")
            }
            Failure::Redirected => format!(
                "{label} redirected the request to another address, which Roundhouse doesn't follow. Edit \
                 its URL in Settings › MCP servers."
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

    /// The answer to a request whose body is `asked`, where `label` is what the person calls the server.
    pub(super) fn answer(self, label: &str, asked: &[u8]) -> Response<AnswerBody> {
        answer(self.said(label), asked)
    }
}

/// The answer to a request whose body is `asked`, to a server the hub hasn't set the worker with, as when the person has
/// removed it, or the hub can't read its key, so nothing is known of what it's called.
pub(super) fn not_set(asked: &[u8]) -> Response<AnswerBody> {
    let said =
        "Roundhouse can't use this MCP server now. It may have been removed, or its key or sign-in can't be read.";
    answer(said.to_string(), asked)
}

/// The answer to a request whose body is `asked`, saying `said`: a JSON-RPC error for a request with an ID, and otherwise
/// a status that says it's the server that failed, which isn't one a CLI takes for the session's token being rejected.
fn answer(said: String, asked: &[u8]) -> Response<AnswerBody> {
    match Message::of(asked).and_then(|message| message.id) {
        Some(id) => {
            let error = json!({ "code": INTERNAL_ERROR, "message": said });
            let body = json!({ "jsonrpc": "2.0", "id": id, "error": error }).to_string();
            whole(StatusCode::OK, HeaderValue::from_static("application/json"), body)
        }
        None => whole(StatusCode::BAD_GATEWAY, HeaderValue::from_static("text/plain; charset=utf-8"), said),
    }
}
