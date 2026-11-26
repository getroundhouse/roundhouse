use super::Asked;
use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use hyper::{Method, StatusCode};
use parking_lot::Mutex;
use protocol::model::{SessionId, ToolServerId, ToolServerPlace};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Which of a 2025 server's sessions a request is of, which the server gave the agent as it connected.
const MCP_SESSION: HeaderName = HeaderName::from_static("mcp-session-id");

/// What an agent's CLI connects to a server with: the one on protocol `2026-07-28`, and the other on those before.
/// Claude tries the first with either kind, and then the second where it's refused.
const HANDSHAKES: [&str; 2] = ["server/discover", "initialize"];

/// Where each chat's agent last connected to each server. Once the person has moved the server to another URL, what the
/// agent sends is let through to it only as the agent connects to it there, and never with a session the one before
/// gave it, so nothing that was written for one server reaches another. A call has no session on `2026-07-28`, nor of a
/// server that gives none, so once the agent has connected again, one it wrote before that can't be told from one it
/// wrote after.
///
/// An agent can have several connections to a server, as each of Codex's threads has, so a session given by the server
/// before is kept out after another connection has connected to it where it is.
#[derive(Default)]
pub(super) struct Connections {
    by_agent: Mutex<HashMap<(SessionId, ToolServerId), Connection>>,
}

struct Connection {
    /// Where the server was.
    place: ToolServerPlace,
    /// The sessions it has given the agent there.
    sessions: HashSet<HeaderValue>,
    /// Whether the agent has been refused since, so the log says so once.
    cut_off: bool,
}

/// Why a request is let through.
#[derive(Clone, Copy)]
pub(super) enum LetIn {
    /// The agent is connected to the server where it is, or it's the first the worker hears of it.
    AsConnected,
    /// It connects to where the server has been moved.
    Connecting,
}

/// A request that isn't. `first` is whether it's the agent's first since it was cut off.
pub(super) struct CutOff {
    pub(super) first: bool,
}

impl Connections {
    /// Whether `asked`, of the server `id`, which is at `place`, is let through to it.
    pub(super) fn lets_in(
        &self,
        id: &ToolServerId,
        place: &ToolServerPlace,
        asked: &Asked<'_>,
    ) -> Result<LetIn, CutOff> {
        let mut by_agent = self.by_agent.lock();
        let Some(connection) = by_agent.get_mut(&key(id, asked)) else { return Ok(LetIn::AsConnected) };
        let session = asked.headers.get(MCP_SESSION);
        if connection.place == *place && session.is_none_or(|session| connection.sessions.contains(session)) {
            return Ok(LetIn::AsConnected);
        }
        if connects(asked) {
            return Ok(LetIn::Connecting);
        }
        Err(CutOff { first: !std::mem::replace(&mut connection.cut_off, true) })
    }

    /// The server `id`, at `place`, answered `asked`, which was `let_in`, with `status` and `headers`. One that refuses
    /// to be connected to leaves the agent where it was.
    pub(super) fn answered(
        &self,
        id: &ToolServerId,
        place: &ToolServerPlace,
        asked: &Asked<'_>,
        let_in: LetIn,
        (status, headers): (StatusCode, &HeaderMap),
    ) {
        if !status.is_success() {
            return;
        }
        let mut by_agent = self.by_agent.lock();
        let here = || Connection { place: place.clone(), sessions: HashSet::new(), cut_off: false };
        let connection = by_agent.entry(key(id, asked)).or_insert_with(here);
        match let_in {
            LetIn::Connecting if connection.place != *place => *connection = here(),
            LetIn::Connecting => connection.cut_off = false,
            // The answer of one that was moved as it answered.
            LetIn::AsConnected if connection.place != *place => return,
            LetIn::AsConnected => {}
        }
        let sessions = [asked.headers.get(MCP_SESSION), headers.get(MCP_SESSION)];
        connection.sessions.extend(sessions.into_iter().flatten().cloned());
    }

    /// Where the agent of `session_id` last connected to the server `id`.
    pub(super) fn place(&self, session_id: &SessionId, id: &ToolServerId) -> Option<ToolServerPlace> {
        Some(self.by_agent.lock().get(&(session_id.clone(), id.clone()))?.place.clone())
    }

    /// An agent of `session_id` starts, and has connected to nothing.
    pub(super) fn forget(&self, session_id: &SessionId) {
        self.by_agent.lock().retain(|(of, _), _| of != session_id);
    }
}

fn key(id: &ToolServerId, asked: &Asked<'_>) -> (SessionId, ToolServerId) {
    (asked.session_id.clone(), id.clone())
}

/// Whether `asked` has an agent connect to a server afresh, in no session of one.
fn connects(asked: &Asked<'_>) -> bool {
    let afresh = *asked.method == Method::POST && !asked.headers.contains_key(MCP_SESSION);
    afresh && method_of(&asked.body).is_some_and(|method| HANDSHAKES.contains(&&*method))
}

#[derive(Deserialize)]
struct Message<'a> {
    #[serde(borrow)]
    method: Cow<'a, str>,
}

/// What the message `body` asks for, in JSON-RPC's words.
pub(super) fn method_of(body: &[u8]) -> Option<Cow<'_, str>> {
    Some(serde_json::from_slice::<Message<'_>>(body).ok()?.method)
}
