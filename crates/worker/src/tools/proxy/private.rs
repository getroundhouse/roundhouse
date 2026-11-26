//! What connects for a request that carries a sign-in's access token over plain `http`: only to an address that's
//! private, as [`is_private`] says, so nobody on the internet between can read the token. The address is checked as
//! it's connected to, rather than looked up beforehand, since a name can come to another address by then, and on
//! another machine, such as a laptop on another network, to another again.

use hyper::Uri;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::connect::dns::{GaiResolver, Name};
use protocol::tool_server_http::is_private;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::vec;
use thiserror::Error;
use tower_service::Service;

type BoxError = Box<dyn Error + Send + Sync>;
type Connecting<T> = Pin<Box<dyn Future<Output = Result<T, BoxError>> + Send>>;

/// Why a request that carries a sign-in wasn't sent: its server's address isn't private, so nothing was connected to.
#[derive(Debug, Error)]
#[error("the server's address isn't private, so its sign-in isn't sent over http")]
pub(super) struct NotPrivate;

/// Where a name's addresses come from.
#[derive(Clone)]
pub(super) enum Lookup {
    /// The system's, as hyper-util's own connector looks a name up.
    System(GaiResolver),
    /// Made up for a test, which can't otherwise have a name come to an address on the internet without connecting
    /// to one.
    #[cfg(test)]
    Fixed(std::sync::Arc<std::collections::HashMap<&'static str, Vec<IpAddr>>>),
}

impl Default for Lookup {
    fn default() -> Self {
        Lookup::System(GaiResolver::new())
    }
}

/// Connects only to a private address. A host that's an address is checked here, since hyper-util connects to one
/// without looking it up, and a name comes to its private addresses alone, through [`PrivateAddresses`].
#[derive(Clone)]
pub(super) struct PrivateOnly(pub(super) HttpConnector<PrivateAddresses>);

impl Service<Uri> for PrivateOnly {
    type Response = <HttpConnector<PrivateAddresses> as Service<Uri>>::Response;
    type Error = BoxError;
    type Future = Connecting<Self::Response>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        self.0.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, to: Uri) -> Self::Future {
        if address_of(&to).is_some_and(|address| !is_private(address)) {
            return Box::pin(async { Err(NotPrivate.into()) });
        }
        let connecting = self.0.call(to);
        Box::pin(async move { connecting.await.map_err(Into::into) })
    }
}

/// The address `uri`'s host is, if it's one, written as hyper-util reads it, with an IPv6 one in brackets.
fn address_of(uri: &Uri) -> Option<IpAddr> {
    let host = uri.host()?;
    let host = host.strip_prefix('[').and_then(|host| host.strip_suffix(']')).unwrap_or(host);
    host.parse().ok()
}

/// A name's private addresses, and none of its others. A name with none fails with [`NotPrivate`].
#[derive(Clone, Default)]
pub(super) struct PrivateAddresses(pub(super) Lookup);

impl Service<Name> for PrivateAddresses {
    type Response = vec::IntoIter<SocketAddr>;
    type Error = BoxError;
    type Future = Connecting<Self::Response>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let found = self.0.of(name);
        Box::pin(async move {
            let private: Vec<_> = found.await?.into_iter().filter(|address| is_private(address.ip())).collect();
            if private.is_empty() { Err(NotPrivate.into()) } else { Ok(private.into_iter()) }
        })
    }
}

impl Lookup {
    /// Every address `name` comes to, with no port, which the connector gives each.
    fn of(&self, name: Name) -> Connecting<Vec<SocketAddr>> {
        match self {
            Lookup::System(resolver) => {
                let found = resolver.clone().call(name);
                Box::pin(async move { Ok(found.await?.collect()) })
            }
            #[cfg(test)]
            Lookup::Fixed(names) => {
                let found = names.get(name.as_str()).map(|found| found.iter().map(|address| (*address, 0).into()));
                let found = found.map(Iterator::collect).ok_or_else(|| "no such name".into());
                Box::pin(async move { found })
            }
        }
    }
}
