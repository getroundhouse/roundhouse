use http::Response;
use http::header::{CONTENT_TYPE, HeaderMap};
use http_body_util::{BodyExt as _, Collected, LengthLimitError, Limited};
use hyper::body::Incoming;
use protocol::client::tool_server_checks::NoToolList;
use serde::Deserialize;
use serde_json::Value;

use super::events::Events;

/// The most read of an answer. A server's tools, with all they say of themselves, are some hundreds of KB where it has
/// many.
pub(super) const LONGEST_ANSWER: usize = 4 * 1024 * 1024;

/// What JSON-RPC answers a request with.
pub(super) enum Said {
    Result(Value),
    Error,
    /// Not JSON-RPC, or no answer to the request.
    Nothing,
}

/// A message, as far as it's an answer.
#[derive(Deserialize)]
struct Message {
    jsonrpc: String,
    /// A result can be `null`, which is one all the same.
    #[serde(default, deserialize_with = "some")]
    result: Option<Value>,
    error: Option<Value>,
}

/// What `response` says, as JSON or as an event of a stream, which is read no further than that.
pub(super) async fn said(response: Response<Incoming>) -> Result<Said, NoToolList> {
    let (head, body) = response.into_parts();
    let body = Limited::new(body, LONGEST_ANSWER);
    if is_stream(&head.headers) { in_stream(body).await } else { whole(body).await }
}

async fn whole(body: Limited<Incoming>) -> Result<Said, NoToolList> {
    let body = body.collect().await.map(Collected::to_bytes).map_err(|e| unread(&*e))?;
    Ok(answer_in(&body).unwrap_or(Said::Nothing))
}

async fn in_stream(mut body: Limited<Incoming>) -> Result<Said, NoToolList> {
    let mut events = Events::default();
    while let Some(frame) = body.frame().await {
        let Ok(bytes) = frame.map_err(|e| unread(&*e))?.into_data() else { continue };
        for byte in bytes {
            // Any other is a message of the server's own, such as its progress.
            if let Some(said) = events.take(byte).and_then(|data| answer_in(&data)) {
                return Ok(said);
            }
        }
    }
    Err(NoToolList::Lost)
}

/// What `body` says, if it's JSON-RPC's answer to the request, which is the only one on its connection.
fn answer_in(body: &[u8]) -> Option<Said> {
    let Message { jsonrpc, result, error } = serde_json::from_slice(body).ok()?;
    if jsonrpc != "2.0" {
        return None;
    }
    match (result, error) {
        (_, Some(_)) => Some(Said::Error),
        (Some(result), None) => Some(Said::Result(result)),
        (None, None) => None,
    }
}

fn is_stream(headers: &HeaderMap) -> bool {
    let said = headers.get(CONTENT_TYPE).and_then(|said| said.to_str().ok()).unwrap_or_default();
    said.split(';').next().is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
}

fn unread(error: &(dyn std::error::Error + 'static)) -> NoToolList {
    if error.is::<LengthLimitError>() { NoToolList::TooLong } else { NoToolList::Lost }
}

fn some<'de, D: serde::Deserializer<'de>>(from: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(from).map(Some)
}
