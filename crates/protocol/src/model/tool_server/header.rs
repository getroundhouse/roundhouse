use std::fmt;

use http::HeaderName;
use serde::{Deserialize, Serialize};

/// The HTTP header a tool server's key is sent in, such as `Authorization`, spelled as the person spelled it, though
/// HTTP reads any case as the same.
///
/// Only [`ToolServerHeader::typed`] checks it, as [`ToolServerName`](super::ToolServerName) says of a name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct ToolServerHeader(String);

/// Why what a person typed can't be the header a key is sent in. It never holds the text, since that can be the key,
/// pasted in the wrong field.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolServerHeaderError {
    #[error("a key needs a header to be sent in, such as Authorization")]
    Empty,
    #[error("a header's name is at most {most} characters")]
    TooLong { most: usize },
    #[error("a header's name is letters, digits and punctuation, with no spaces")]
    NotAHeader,
    #[error("that header is part of how a server is reached, so a key can't be sent in it")]
    Reserved,
}

/// The headers whoever sends a request to a tool server sets itself, so a key in one would break the request, or be
/// written over: HTTP's, for the connection and for what it carries, and those of MCP's Streamable HTTP transport. In
/// lowercase. *Unconfirmed:* any but `Host`, `Content-Length`, `Connection`, `Transfer-Encoding` and MCP's.
const RESERVED: [&str; 18] = [
    "accept",
    "accept-encoding",
    "connection",
    "content-encoding",
    "content-length",
    "content-type",
    "expect",
    "host",
    "keep-alive",
    "last-event-id",
    "mcp-method",
    "mcp-name",
    "mcp-protocol-version",
    "mcp-session-id",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// What the headers that are only for a proxy on the way start with, which it doesn't pass on to the server. In
/// lowercase.
const FOR_A_PROXY: &str = "proxy-";

const AUTHORIZATION: &str = "Authorization";

impl ToolServerHeader {
    /// `Authorization`, which a signed-in server's access token is sent in.
    pub fn authorization() -> Self {
        Self(AUTHORIZATION.into())
    }

    /// Whether it's that one, however it's spelled, whose value is a scheme, a space, and what the scheme is given.
    pub fn is_authorization(&self) -> bool {
        self.0.eq_ignore_ascii_case(AUTHORIZATION)
    }

    /// Longer than the name of any header a server reads a key from.
    pub const MOST_BYTES: usize = 128;

    /// The name of a header as a person typed it, trimmed.
    pub fn typed(text: &str) -> Result<Self, ToolServerHeaderError> {
        let header = text.trim();
        if header.is_empty() {
            return Err(ToolServerHeaderError::Empty);
        }
        if header.len() > Self::MOST_BYTES {
            return Err(ToolServerHeaderError::TooLong { most: Self::MOST_BYTES });
        }
        // Read as lowercase.
        let read = HeaderName::from_bytes(header.as_bytes()).map_err(|_| ToolServerHeaderError::NotAHeader)?;
        if RESERVED.contains(&read.as_str()) || read.as_str().starts_with(FOR_A_PROXY) {
            return Err(ToolServerHeaderError::Reserved);
        }
        Ok(Self(header.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolServerHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
