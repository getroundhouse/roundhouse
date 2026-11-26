use http::header::{HeaderMap, LOCATION};
use http::uri::{PathAndQuery, Scheme};
use http::{StatusCode, Uri};

/// More than a server that adds a slash to its path, or has moved once, needs, and few enough that one going in circles
/// ends soon.
pub const MOST_REDIRECTS_WITHIN: usize = 3;

/// What an answer says of where its request is to go instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Redirected {
    Not,
    /// Within its origin, with the same method and body.
    Within(Uri),
    /// Anywhere else, or as another request, so it isn't followed.
    Elsewhere,
}

/// What an answer of `status`, with `headers`, says of a request sent to `from`. One that keeps the method and the body
/// is followed within its origin, so the key only ever goes to the origin it was given for.
pub fn redirected(from: &Uri, status: StatusCode, headers: &HeaderMap) -> Redirected {
    match status {
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => {
            at_same_origin(from, headers).map_or(Redirected::Elsewhere, Redirected::Within)
        }
        status if status.is_redirection() => Redirected::Elsewhere,
        _ => Redirected::Not,
    }
}

/// A path that begins with no slash isn't taken, which no MCP server is known to answer with.
fn at_same_origin(from: &Uri, headers: &HeaderMap) -> Option<Uri> {
    let location = headers.get(LOCATION)?.to_str().ok()?;
    // Two slashes begin another host's name.
    if location.starts_with('/') && !location.starts_with("//") {
        let path: PathAndQuery = location.parse().ok()?;
        let mut parts = from.clone().into_parts();
        parts.path_and_query = Some(path);
        return Uri::from_parts(parts).ok();
    }
    let to: Uri = location.parse().ok()?;
    let origin = |uri: &Uri| Some((uri.scheme()?.clone(), uri.host()?.to_ascii_lowercase(), port(uri)));
    (origin(&to)? == origin(from)?).then_some(to)
}

fn port(uri: &Uri) -> Option<u16> {
    uri.port_u16().or_else(|| match uri.scheme() {
        Some(scheme) if *scheme == Scheme::HTTP => Some(80),
        Some(scheme) if *scheme == Scheme::HTTPS => Some(443),
        Some(_) | None => None,
    })
}
