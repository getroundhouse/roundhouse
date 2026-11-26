use std::convert::Infallible;
use std::error::Error;
use std::future::{Ready, ready};
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use http::uri::Scheme;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper_rustls::{ConfigBuilderExt as _, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::connect::dns::Name;
use hyper_util::rt::TokioExecutor;
use rustls::ClientConfig;
use rustls::crypto::ring;
use tokio::sync::OnceCell;
use tower_service::Service;

/// TLS on ring, trusting the system's certificates.
#[derive(Clone)]
pub(crate) struct Tls(ClientConfig);

impl Tls {
    async fn load() -> Result<Self, Box<dyn Error + Send + Sync>> {
        tokio::task::spawn_blocking(|| {
            let builder = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()));
            let config = builder.with_safe_default_protocol_versions()?.with_native_roots()?;
            Ok(Self(config.with_no_client_auth()))
        })
        .await?
    }
}

/// Comes to the addresses already vetted, and no others, whatever the name.
#[derive(Clone)]
struct Vetted(Vec<SocketAddr>);

impl Service<Name> for Vetted {
    type Response = vec::IntoIter<SocketAddr>;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Infallible>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Name) -> Self::Future {
        ready(Ok(self.0.clone().into_iter()))
    }
}

/// Sends `request` over a connection of its own, to one of `addresses`, so nothing else is connected to, and no call
/// uses another's connection. The connector connects to a host that's an address without asking what it comes to, and
/// `addresses` is then that address alone.
pub(crate) async fn send(
    tls: &OnceCell<Tls>,
    addresses: Vec<SocketAddr>,
    request: Request<Full<Bytes>>,
) -> Result<Response<Incoming>, Box<dyn Error + Send + Sync>> {
    let mut connector = HttpConnector::new_with_resolver(Vetted(addresses));
    connector.enforce_http(false);
    let mut client = Client::builder(TokioExecutor::new());
    client.pool_max_idle_per_host(0);
    if request.uri().scheme() == Some(&Scheme::HTTP) {
        return Ok(client.build(connector).request(request).await?);
    }
    let Tls(config) = tls.get_or_try_init(Tls::load).await?.clone();
    let https = HttpsConnectorBuilder::new().with_tls_config(config).https_only().enable_http1();
    Ok(client.build(https.wrap_connector(connector)).request(request).await?)
}
