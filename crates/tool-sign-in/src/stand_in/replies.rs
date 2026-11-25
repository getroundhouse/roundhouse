use http::header::{CONTENT_TYPE, LOCATION, SET_COOKIE};
use http::{HeaderValue, Response};
use http_body_util::Full;
use hyper::body::Bytes;
use serde_json::{Value, json};

/// What the stand-in answers a request with. Each sets a cookie, which no client should send back.
pub(super) type Reply = Response<Full<Bytes>>;

pub(super) fn said(status: u16, body: &Value) -> Reply {
    reply(status, Some(body.to_string()))
}

/// `status`, with no more, but for a redirect's, which says it's at `/moved`.
pub(super) fn status(status: u16) -> Reply {
    let mut reply = reply(status, None);
    if (300..400).contains(&status) {
        reply.headers_mut().insert(LOCATION, HeaderValue::from_static("/moved"));
    }
    reply
}

/// One of OAuth's errors, such as `invalid_grant`, with a description, which the client leaves out of what it says.
pub(super) fn oauth_error(error: &str) -> Reply {
    said(400, &json!({"error": error, "error_description": "made up"}))
}

fn reply(status: u16, body: Option<String>) -> Reply {
    let reply = Response::builder().status(status).header(CONTENT_TYPE, "application/json");
    reply.header(SET_COOKIE, "made-up=1").body(Full::new(body.unwrap_or_default().into())).unwrap()
}
