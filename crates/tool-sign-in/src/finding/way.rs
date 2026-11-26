use std::fmt;

use protocol::model::ToolScopes;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::ServerUrl;

/// What an authorization server is called, by its own word and the server's, letter for letter, which is how it's told
/// from another.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Issuer(String);

impl Issuer {
    pub(crate) fn new(issuer: impl Into<String>) -> Self {
        Self(issuer.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Issuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The clients an authorization server takes that Roundhouse could be, of which the first is the one MCP has it be.
#[derive(Clone, Debug)]
pub(crate) enum Takes {
    /// Those with a document that describes them.
    Documents,
    /// Those that register at `endpoint`.
    Registered { endpoint: Url },
}

/// How a server is signed in to, as it and its authorization server say.
#[derive(Clone, Debug)]
pub struct Way {
    pub(crate) server: ServerUrl,
    /// The server, as its metadata names it.
    pub(crate) resource: String,
    pub(crate) issuer: Issuer,
    pub(crate) authorization_endpoint: Url,
    pub(crate) token_endpoint: Url,
    pub(crate) takes: Takes,
    /// Whether the authorization server says who it is as it sends the person back, as RFC 9207 has it.
    pub(crate) says_iss: bool,
    pub(crate) scopes: ToolScopes,
    /// Whether the authorization server has the scope that asks for a refresh token.
    pub(crate) offline: bool,
}

impl Way {
    /// The authorization server, which a sign-in is good only with.
    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    /// What to ask to be allowed: what the server said as it asked for a sign-in, or else what its metadata says.
    pub fn scopes(&self) -> &ToolScopes {
        &self.scopes
    }
}
