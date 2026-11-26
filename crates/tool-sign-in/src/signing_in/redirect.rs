use std::net::Ipv4Addr;

use thiserror::Error;
use url::{Host, Url};

/// Where an authorization server sends the person back: the app, listening on 127.0.0.1, as RFC 8252, section 7.3, has a
/// native app. It's one of those the client document lists, which are all there, letter for letter but for its port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedirectUri(String);

/// What was given isn't where an app on this machine listens.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("that isn't where an app on this machine listens, such as http://127.0.0.1:17867/callback")]
pub struct NotARedirect;

impl RedirectUri {
    pub fn parse(text: &str) -> Result<Self, NotARedirect> {
        let url = Url::parse(text).map_err(|_| NotARedirect)?;
        let here = url.scheme() == "http" && url.host() == Some(Host::Ipv4(Ipv4Addr::LOCALHOST));
        let bare = url.username().is_empty() && url.password().is_none();
        // The authorization server adds its own.
        let ends_at_path = url.query().is_none() && url.fragment().is_none();
        if here && bare && ends_at_path { Ok(Self(text.to_string())) } else { Err(NotARedirect) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
