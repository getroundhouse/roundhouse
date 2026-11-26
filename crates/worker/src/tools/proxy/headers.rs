use hyper::header::{
    ACCEPT, ACCEPT_LANGUAGE, ALLOW, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_TYPE, HeaderMap,
    HeaderName, RETRY_AFTER, USER_AGENT,
};

/// What each of MCP's own headers begins with, such as `Mcp-Session-Id`, `MCP-Protocol-Version`, and the `Mcp-Param-…`
/// a tool's arguments can be sent in.
const MCP: &str = "mcp-";

/// What a server is sent of a request's headers, beside MCP's own. `traceparent` and `tracestate` are the trace Claude
/// sends with a call. Naming what's passed on keeps everything else back, such as the session's token, a cookie, and
/// what's only about one connection, such as `Connection` and `Transfer-Encoding`. With no `Accept-Encoding`, an answer
/// comes as it is.
const TO_SERVER: [HeaderName; 8] = [
    ACCEPT,
    ACCEPT_LANGUAGE,
    CONTENT_ENCODING,
    CONTENT_TYPE,
    HeaderName::from_static("last-event-id"),
    HeaderName::from_static("traceparent"),
    HeaderName::from_static("tracestate"),
    USER_AGENT,
];

/// What an agent is sent of an answer's headers, beside MCP's own, which keeps back the same, and `WWW-Authenticate`,
/// `Set-Cookie` and `Location`.
const TO_AGENT: [HeaderName; 6] = [ALLOW, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_TYPE, RETRY_AFTER];

pub(super) fn to_server(headers: &HeaderMap) -> HeaderMap {
    passed_on(headers, &TO_SERVER)
}

pub(super) fn to_agent(headers: &HeaderMap) -> HeaderMap {
    passed_on(headers, &TO_AGENT)
}

fn passed_on(headers: &HeaderMap, named: &[HeaderName]) -> HeaderMap {
    let passes = |name: &HeaderName| named.contains(name) || name.as_str().starts_with(MCP);
    let kept = headers.iter().filter(|(name, _)| passes(name));
    // Adding each, so one sent in several lines stays so.
    let mut all = HeaderMap::new();
    for (name, value) in kept {
        all.append(name, value.clone());
    }
    all
}
