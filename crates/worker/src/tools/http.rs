//! The tools over HTTP: MCP's Streamable HTTP, as protocol `2026-07-28` has it, with each request answered as JSON.
//! Only from this machine, and only with a token the hub gave one of this worker's agents.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    ALLOW, AUTHORIZATION, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue, ORIGIN, WWW_AUTHENTICATE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use protocol::model::SessionId;
use protocol::worker::ToolsToken;
use tokio::net::TcpStream;
use tracing::debug;

use super::mcp::{self, Code, Headers, Reply, Said};
use super::{PATH, Tools};

#[cfg(test)]
mod tests;

/// Far more than any call to these tools carries, which is a few kilobytes.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// An agent on this machine sends a request's headers at once, so a connection that hasn't is closed, rather than
/// holding a descriptor the worker's agents, git and scripts need too.
const HEADERS_TIMEOUT: Duration = Duration::from_secs(30);

const VERSION_HEADER: HeaderName = HeaderName::from_static("mcp-protocol-version");
const METHOD_HEADER: HeaderName = HeaderName::from_static("mcp-method");
const NAME_HEADER: HeaderName = HeaderName::from_static("mcp-name");

pub(super) async fn serve_connection(stream: TcpStream, tools: Arc<Tools>, port: u16) {
    let service = service_fn(move |request: Request<Incoming>| {
        let tools = tools.clone();
        async move {
            let (parts, body) = request.into_parts();
            Ok::<_, Infallible>(respond(&tools, port, &parts.method, &parts.uri, &parts.headers, body).await)
        }
    });
    let mut http = http1::Builder::new();
    http.timer(TokioTimer::new()).header_read_timeout(HEADERS_TIMEOUT);
    if let Err(e) = http.serve_connection(TokioIo::new(stream), service).await {
        debug!("a connection to the worker's tools ended: {e}");
    }
}

/// Refuses what isn't from one of this worker's agents before reading the body, then answers the message in it.
async fn respond<B>(
    tools: &Tools,
    port: u16,
    method: &Method,
    uri: &hyper::Uri,
    headers: &HeaderMap,
    body: B,
) -> Response<Full<Bytes>>
where
    B: hyper::body::Body,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    // A web page can reach 127.0.0.1 too, under a name of its own that leads here, and says where it's from.
    if !is_local(headers.get(HOST), port) || headers.contains_key(ORIGIN) {
        return empty(StatusCode::FORBIDDEN);
    }
    if uri.path() != PATH {
        return empty(StatusCode::NOT_FOUND);
    }
    if method != Method::POST {
        let mut refused = empty(StatusCode::METHOD_NOT_ALLOWED);
        refused.headers_mut().insert(ALLOW, HeaderValue::from_static("POST"));
        return refused;
    }
    let Some(session_id) = session_for(tools, headers) else {
        let mut refused = empty(StatusCode::UNAUTHORIZED);
        refused.headers_mut().insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return refused;
    };
    let body = match Limited::new(body, MAX_BODY_BYTES).collect().await {
        Ok(body) => body.to_bytes(),
        Err(e) if e.is::<LengthLimitError>() => return empty(StatusCode::PAYLOAD_TOO_LARGE),
        Err(_) => return empty(StatusCode::BAD_REQUEST),
    };
    match mcp::answer(&tools.asks, &session_id, &headers_of(headers), &body).await {
        Reply::Accepted => empty(StatusCode::ACCEPTED),
        Reply::Answer(answer) => {
            let status = match &answer.outcome {
                Ok(_) => StatusCode::OK,
                Err(error) => status_of(error.code),
            };
            let mut response = Response::new(Full::new(Bytes::from(answer.to_json().to_string())));
            *response.status_mut() = status;
            response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response
        }
    }
}

/// Whether the request was sent to this server by its own address, as the agents' config names it.
fn is_local(host: Option<&HeaderValue>, port: u16) -> bool {
    let Some(host) = host.and_then(|host| host.to_str().ok()) else { return false };
    host.eq_ignore_ascii_case(&format!("127.0.0.1:{port}")) || host.eq_ignore_ascii_case(&format!("localhost:{port}"))
}

fn session_for(tools: &Tools, headers: &HeaderMap) -> Option<SessionId> {
    let said = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = said.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    tools.session_for(&ToolsToken::new(token.trim()))
}

fn headers_of(headers: &HeaderMap) -> Headers {
    Headers {
        version: plain(headers, &VERSION_HEADER),
        method: plain(headers, &METHOD_HEADER),
        name: match plain(headers, &NAME_HEADER) {
            Said::Value(name) => decoded(&name),
            said @ (Said::Nothing | Said::Unreadable) => said,
        },
    }
}

fn plain(headers: &HeaderMap, name: &HeaderName) -> Said {
    match headers.get(name).map(HeaderValue::to_str) {
        None => Said::Nothing,
        Some(Ok(value)) => Said::Value(value.to_string()),
        Some(Err(_)) => Said::Unreadable,
    }
}

/// A name that isn't plain ASCII comes as `=?base64?…?=`, Streamable HTTP's way of carrying it in a header.
fn decoded(name: &str) -> Said {
    let Some(encoded) = name.strip_prefix("=?base64?").and_then(|rest| rest.strip_suffix("?=")) else {
        return Said::Value(name.to_string());
    };
    match STANDARD.decode(encoded).ok().and_then(|bytes| String::from_utf8(bytes).ok()) {
        Some(name) => Said::Value(name),
        None => Said::Unreadable,
    }
}

/// Streamable HTTP's status for each error: a request it can't take is a bad one, and a method it doesn't have isn't
/// found. An error from a method it has is that method's answer.
fn status_of(code: Code) -> StatusCode {
    match code {
        Code::Parse | Code::InvalidRequest | Code::HeaderMismatch | Code::UnsupportedVersion => StatusCode::BAD_REQUEST,
        Code::MethodNotFound => StatusCode::NOT_FOUND,
        Code::InvalidParams | Code::Internal | Code::MissingCapability => StatusCode::OK,
    }
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = status;
    response
}
