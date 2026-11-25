use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use protocol::model::ToolServerId;

const PREFIX: &str = "/tool-servers/";

/// What an ID keeps as it is in a path, which is all a UUID has.
const KEPT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-');

/// The server's path on the worker, by its ID, so it's the same whatever else the hub sends of the server.
pub(super) fn path_of(id: &ToolServerId) -> String {
    format!("{PREFIX}{}", utf8_percent_encode(id.as_str(), KEPT))
}

/// The server a path is, if it's one's and nothing under it.
pub(in super::super) fn server_in(path: &str) -> Option<ToolServerId> {
    let segment = path.strip_prefix(PREFIX).filter(|rest| !rest.is_empty() && !rest.contains('/'))?;
    let id = percent_decode_str(segment).decode_utf8().ok()?;
    Some(ToolServerId::from(id.as_ref()))
}
