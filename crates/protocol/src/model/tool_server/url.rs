use std::fmt;

use http::Uri;
use http::uri::Authority;
use serde::{Deserialize, Serialize};

use super::ToolServerPlace;

/// Where a tool server is, such as `https://mcp.example.com/mcp`: an `http` or `https` address of a host, with no
/// username or password in it. It's kept and sent as it is, its query too, though that can hold a key, as
/// [`ToolServerUrl::looks_keyed`] says. This file and [`ToolServerPlace`]'s are the one home of which text can be a
/// URL, and of which two are one server's.
///
/// Only [`ToolServerUrl::typed`] checks it, as [`ToolServerName`](super::ToolServerName) says of a name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct ToolServerUrl(String);

/// Why what a person typed can't be a tool server's URL. It never holds the text, since that can hold a password.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolServerUrlError {
    #[error("an MCP server needs a URL")]
    Empty,
    #[error("an MCP server's URL is at most {most} characters")]
    TooLong { most: usize },
    #[error("an MCP server's URL starts with http:// or https://")]
    NotHttp,
    #[error("an MCP server's URL can't hold a username or a password, so use a key instead")]
    HoldsCredential,
    #[error("an MCP server's URL can't have a # in it")]
    HasFragment,
    #[error("that isn't a URL a server can be reached at")]
    NotAUrl,
}

impl ToolServerUrl {
    /// Longer than any server's address, and short enough to send every client.
    pub const MOST_BYTES: usize = 2048;

    /// A URL as a person typed it, trimmed, and otherwise as it was. A username or a password is refused rather than
    /// cut out, since the person meant it to be sent. A fragment is too, since none is ever sent to a server.
    pub fn typed(text: &str) -> Result<Self, ToolServerUrlError> {
        let url = text.trim();
        if url.is_empty() {
            return Err(ToolServerUrlError::Empty);
        }
        if url.len() > Self::MOST_BYTES {
            return Err(ToolServerUrlError::TooLong { most: Self::MOST_BYTES });
        }
        let Some((scheme, rest)) = url.split_once("://") else { return Err(ToolServerUrlError::NotHttp) };
        if !["http", "https"].iter().any(|http| scheme.eq_ignore_ascii_case(http)) {
            return Err(ToolServerUrlError::NotHttp);
        }
        // Before it's parsed, so what can't be parsed for what's in a password is still said to hold one.
        let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
        if authority.contains('@') {
            return Err(ToolServerUrlError::HoldsCredential);
        }
        // `http` drops one without a word.
        if url.contains('#') {
            return Err(ToolServerUrlError::HasFragment);
        }
        let parsed: Uri = url.parse().map_err(|_| ToolServerUrlError::NotAUrl)?;
        if !parsed.authority().is_some_and(can_be_reached) {
            return Err(ToolServerUrlError::NotAUrl);
        }
        Ok(Self(url.to_owned()))
    }

    /// A URL as the hub stored it. Only the hub's store calls this.
    pub fn stored(text: String) -> Self {
        Self(text)
    }

    /// The host a server is at, with its port if it has one, such as `mcp.example.com`, for a row with no room for the
    /// rest. All of one that isn't a URL, which only a hub whose rule differs could have kept.
    pub fn host(&self) -> &str {
        let after_scheme = self.0.split_once("://").map_or(self.as_str(), |(_, rest)| rest);
        after_scheme.split(['/', '?']).next().filter(|host| !host.is_empty()).unwrap_or(&self.0)
    }

    /// Which server it's of.
    pub fn place(&self) -> ToolServerPlace {
        ToolServerPlace::of(self)
    }

    /// What's asked of it, which is all of it that isn't its place: what's after its `?`, where it has one.
    pub fn query(&self) -> Option<&str> {
        self.0.split_once('?').map(|(_, query)| query)
    }

    /// Whether `other` is of the same server, so what that knows the person by stays good: they differ in their queries
    /// at most.
    pub fn is_same_server(&self, other: &ToolServerUrl) -> bool {
        self.place() == other.place()
    }

    /// Whether `typed`, what a person has typed of a URL so far, looks like it has a key in its query, for a client to
    /// warn of, since every client is shown the URL. Nothing tells a key from an option for certain, so it stops
    /// nothing. It goes by an option whose name ends as a key's does and whose value is long enough, with a digit, to
    /// be one.
    pub fn looks_keyed(typed: &str) -> bool {
        let Some((_, query)) = typed.trim().split_once('?') else { return false };
        query.split('&').filter_map(|option| option.split_once('=')).any(|(name, value)| {
            let name: String = name.chars().filter(|c| !['_', '-'].contains(c)).collect::<String>().to_lowercase();
            KEYS_NAMES_END.iter().any(|end| name.ends_with(end))
                && value.chars().count() >= LEAST_KEY_CHARS
                && value.chars().any(|c| c.is_ascii_digit())
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How the name of a query's option that holds a key ends, in lowercase, with no `_` or `-`.
const KEYS_NAMES_END: [&str; 4] = ["apikey", "token", "secret", "password"];

/// The shortest value that's taken for a key.
const LEAST_KEY_CHARS: usize = 16;

/// Whether `authority`, with no username in it, is a host, with a port a server can be on or none. `http` takes any
/// digits, or none, as a port.
fn can_be_reached(authority: &Authority) -> bool {
    let port = authority.as_str().strip_prefix(authority.host()).and_then(|rest| rest.strip_prefix(':'));
    let port_is_one = port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port != 0));
    !authority.host().is_empty() && port_is_one
}

impl fmt::Display for ToolServerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
