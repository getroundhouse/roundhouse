use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// An MCP server's address, as a person gives it. One over plain `http` is fetched only where its host is on this
/// machine or a private network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServerUrl(Url);

/// What was given isn't an address an MCP server can have.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("an MCP server's address starts with https:// and has no name, password or # in it")]
pub struct NotAServerUrl;

impl ServerUrl {
    /// `text`, with any spaces around it left out.
    pub fn parse(text: &str) -> Result<Self, NotAServerUrl> {
        let url = Url::parse(text.trim()).map_err(|_| NotAServerUrl)?;
        let bare = url.username().is_empty() && url.password().is_none() && url.fragment().is_none();
        if !matches!(url.scheme(), "https" | "http") || !bare {
            return Err(NotAServerUrl);
        }
        Ok(Self(url))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn url(&self) -> &Url {
        &self.0
    }
}

impl TryFrom<String> for ServerUrl {
    type Error = NotAServerUrl;

    fn try_from(text: String) -> Result<Self, NotAServerUrl> {
        Self::parse(&text)
    }
}

impl From<ServerUrl> for String {
    fn from(server: ServerUrl) -> Self {
        server.0.into()
    }
}
