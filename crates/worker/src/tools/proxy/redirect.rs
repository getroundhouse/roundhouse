use hyper::Uri;
use hyper::header::{HeaderMap, LOCATION};
use hyper::http::uri::{PathAndQuery, Scheme};

/// Where `headers` redirect a request sent to `from`, if it's at the same origin, so the key only ever goes to the
/// origin it was given for. A path that begins with no slash isn't taken, which no MCP server is known to answer with.
pub(super) fn at_same_origin(from: &Uri, headers: &HeaderMap) -> Option<Uri> {
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
