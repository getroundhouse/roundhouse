use http::Uri;
use http::uri::Scheme;
use serde::{Deserialize, Serialize};

use super::ToolServerUrl;

/// Which server a tool server's URL is of, which is all of it but the query: its scheme, host, port and path, as an
/// address of its own. Two URLs are of one server where they have the same. A query only says what's asked of it, such
/// as which of its tools to have.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ToolServerPlace(String);

impl ToolServerPlace {
    /// The scheme and the host are read in any case, and a port that's the scheme's own is as none. Every other
    /// difference is another server's, such as a slash at the path's end, since a server is free to make one of it. A
    /// URL that isn't one, which only a hub whose rule differs could have kept, is its own place.
    pub(super) fn of(url: &ToolServerUrl) -> Self {
        let place = url.as_str().parse::<Uri>().ok().and_then(|uri| place(&uri));
        Self(place.unwrap_or_else(|| url.as_str().to_owned()))
    }
}

fn place(uri: &Uri) -> Option<String> {
    let scheme = uri.scheme()?;
    let host = uri.host()?.to_ascii_lowercase();
    let port = uri.port_u16().or_else(|| own_port(scheme))?;
    Some(format!("{scheme}://{host}:{port}{}", uri.path()))
}

fn own_port(scheme: &Scheme) -> Option<u16> {
    match scheme.as_str() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}
