use std::io;
use std::net::SocketAddr;

use thiserror::Error;
use tool_server_http::{Names, is_private};
use url::Url;

use protocol::model::ToolServerUrl;

use crate::Failure;

/// Why something isn't fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum NotFetched {
    #[error("it isn't over https")]
    NotHttps,
    #[error("its address has a name, a password or a # in it")]
    NotBare,
    /// It comes only to addresses on this machine or a private network, or kept for something else, and the server
    /// isn't at one. Or the server was at one, and isn't now.
    #[error("it isn't on the internet, or is and the server isn't")]
    Address,
}

/// What's fetched for one server.
pub(crate) struct Reach {
    host: String,
    /// Whether the server is on this machine or a private network, by every address its host comes to.
    private: bool,
}

impl Reach {
    pub(super) async fn of(server: &Url, names: &Names) -> io::Result<Self> {
        let addresses = addresses(names, server).await?;
        let private = !addresses.is_empty() && addresses.iter().all(|address| is_private(address.ip()));
        Ok(Self { host: host(server), private })
    }

    /// The server's own host.
    pub(super) fn host(&self) -> &str {
        &self.host
    }

    /// Whether `url` is of a form that's fetched, or that a browser is sent to.
    pub(crate) fn takes(&self, url: &Url) -> Result<(), NotFetched> {
        let secure = url.scheme() == "https" || (url.scheme() == "http" && self.private);
        if !secure {
            return Err(NotFetched::NotHttps);
        }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(NotFetched::NotBare);
        }
        Ok(())
    }

    /// Those of `addresses`, which `url`'s host comes to, that it's connected to at.
    pub(super) fn vetted(&self, url: &Url, addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, NotFetched> {
        self.takes(url)?;
        // A server taken to be on a private network has to be found there when it's asked too. Otherwise one on the
        // internet, whose name came to a private address for a moment, could have Roundhouse fetch from the network.
        let only_private = url.scheme() == "http" || (self.private && host(url) == self.host);
        let taken = |address: &SocketAddr| if is_private(address.ip()) { self.private } else { !only_private };
        let vetted: Vec<_> = addresses.into_iter().filter(taken).collect();
        if vetted.is_empty() { Err(NotFetched::Address) } else { Ok(vetted) }
    }

    #[cfg(test)]
    pub(super) fn made_up(host: &str, private: bool) -> Self {
        Self { host: host.to_string(), private }
    }
}

/// Every address `url`'s host comes to, or its own where it is one, with `url`'s port.
pub(super) async fn addresses(names: &Names, url: &Url) -> io::Result<Vec<SocketAddr>> {
    names.of(&host(url), url.port_or_known_default().unwrap_or_default()).await
}

/// `url`'s host, which an address that's fetched always has.
pub(super) fn host(url: &Url) -> String {
    url.host_str().unwrap_or_default().to_string()
}

/// `server`'s address, as it's fetched from. `ToolServerUrl::typed` took it, so it reads as one but where it was kept by
/// another build's rule.
pub(crate) fn url_of(server: &ToolServerUrl) -> Result<Url, Failure> {
    let what = "an address that isn't one";
    Url::parse(server.as_str()).map_err(|_| Failure::Unreadable { host: server.host().to_string(), what })
}
