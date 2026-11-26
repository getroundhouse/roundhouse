use std::time::Duration;

use http::header::{HeaderName, HeaderValue};
use http::{Response, StatusCode};
use hyper::body::Incoming;
use protocol::client::tool_server_checks::NoToolList;
use protocol::model::ToolServerUrl;
use protocol::worker::ServerHeader;
use serde_json::Value;
use tokio::time::{Instant, timeout, timeout_at};

use super::answer::Said;
use super::{Calls, said};
use crate::ToolSignIn;
use crate::messages::{Message, VERSION_HEADER, VERSION_WITH_HANDSHAKE, refuses_what_was_asked};

/// The longest a server is asked for its tools, all that's asked of it together. One that's there answers in well under
/// a second, and a person waits for it.
pub(super) const CHECK_WITHIN: Duration = Duration::from_secs(15);

/// How much of [`CHECK_WITHIN`] is kept to end a session in, if the server gave one.
const END_WITHIN: Duration = Duration::from_secs(1);

/// Which of a server's sessions a message is of.
const SESSION_HEADER: HeaderName = HeaderName::from_static("mcp-session-id");

/// A session a server gave, as the headers each of its messages has.
type Session = Vec<(HeaderName, HeaderValue)>;

/// What a server answered a message with.
enum Answered {
    Result { result: Value, session: Option<HeaderValue> },
    Error,
    Neither { status: StatusCode },
}

impl ToolSignIn {
    /// Asks `server` for its tools, within [`CHECK_WITHIN`], with `header` in each request, and gives how many the
    /// first page of them has, or why there's no saying.
    pub async fn list_tools(&self, server: &ToolServerUrl, header: Option<&ServerHeader>) -> Result<u32, NoToolList> {
        let unusable = NoToolList::Unreached { why: None };
        let calls = self.calls_to(server, header).ok_or(unusable)?;
        let mut session = None;
        let by = Instant::now() + CHECK_WITHIN.saturating_sub(END_WITHIN);
        let listed = timeout_at(by, listed(&calls, &mut session)).await.unwrap_or(Err(NoToolList::TimedOut));
        if let Some(session) = session {
            // The server ends one that isn't by itself in time, so nothing rests on this.
            let _ended = timeout(END_WITHIN, calls.delete(&session)).await;
        }
        listed
    }
}

/// How many tools `calls`' server lists, as a client of this version asks, and, where it refuses that for what was
/// asked, as one of the last version with a handshake does, with the session that gave.
async fn listed(calls: &Calls<'_>, session: &mut Option<Session>) -> Result<u32, NoToolList> {
    let listed = match ask(calls, &Message::discover(), &[]).await? {
        Answered::Result { .. } => ask(calls, &Message::list_tools(), &[]).await?,
        Answered::Error => after_handshake(calls, session).await?,
        // What isn't JSON-RPC may be all that a server says to a version it doesn't know.
        Answered::Neither { status } if status.is_success() || refuses_what_was_asked(status.as_u16()) => {
            after_handshake(calls, session).await?
        }
        Answered::Neither { .. } => return Err(NoToolList::NotMcp),
    };
    let Answered::Result { result, .. } = listed else { return Err(NoToolList::WontList) };
    let tools = result.get("tools").and_then(Value::as_array).ok_or(NoToolList::WontList)?;
    Ok(u32::try_from(tools.len()).unwrap_or(u32::MAX))
}

/// What a server answers a request for its tools with, once it has been shaken hands with.
async fn after_handshake(calls: &Calls<'_>, session: &mut Option<Session>) -> Result<Answered, NoToolList> {
    let (result, given) = match ask(calls, &Message::initialize(), &[]).await? {
        Answered::Result { result, session } => (result, session),
        Answered::Error => return Err(NoToolList::WontConnect),
        Answered::Neither { status: StatusCode::NOT_FOUND } => return Err(NoToolList::NotFound),
        Answered::Neither { .. } => return Err(NoToolList::NotMcp),
    };
    // Which is its to choose, as the last it knows.
    let version = result.get("protocolVersion").and_then(|version| HeaderValue::from_str(version.as_str()?).ok());
    let version = version.unwrap_or(HeaderValue::from_static(VERSION_WITH_HANDSHAKE));
    let mut headers = vec![(HeaderName::from_static(VERSION_HEADER), version)];
    if let Some(given) = given {
        headers.push((SESSION_HEADER, given));
        *session = Some(headers.clone());
    }
    // Whatever it answers, what it answers next says more.
    calls.post(&Message::initialized(), &headers).await?;
    ask(calls, &Message::list_tools_after_handshake(), &headers).await
}

/// What `calls`' server answers `message` with, unless its status says why it wouldn't.
async fn ask(
    calls: &Calls<'_>,
    message: &Message,
    headers: &[(HeaderName, HeaderValue)],
) -> Result<Answered, NoToolList> {
    let response = calls.post(message, headers).await?;
    if let Some(refused) = refused(calls, &response) {
        return Err(refused);
    }
    let (status, session) = (response.status(), response.headers().get(SESSION_HEADER).cloned());
    match said(response).await {
        Ok(Said::Result(result)) if status.is_success() => Ok(Answered::Result { result, session }),
        Ok(Said::Error) => Ok(Answered::Error),
        Ok(Said::Result(_) | Said::Nothing) => Ok(Answered::Neither { status }),
        // Its status says as much, such as a site's own page for an address it hasn't.
        Err(_) if !status.is_success() => Ok(Answered::Neither { status }),
        Err(unread) => Err(unread),
    }
}

/// Why its status says the server wouldn't answer, whatever it was asked.
fn refused(calls: &Calls<'_>, response: &Response<Incoming>) -> Option<NoToolList> {
    let status = response.status();
    match (status, calls.keyed()) {
        (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN, true) => {
            Some(NoToolList::KeyRefused { status: status.as_u16() })
        }
        (StatusCode::UNAUTHORIZED, false) => Some(NoToolList::WantsSignInOrKey),
        (StatusCode::FORBIDDEN, false) => Some(NoToolList::Forbidden),
        (StatusCode::TOO_MANY_REQUESTS, _) => Some(NoToolList::TooManyRequests),
        (status, _) if status.is_server_error() => Some(NoToolList::Failing { status: status.as_u16() }),
        _ => None,
    }
}
