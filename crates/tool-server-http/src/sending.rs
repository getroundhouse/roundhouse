use std::convert::Infallible;
use std::error::Error;
use std::future::{Ready, ready};
use std::net::SocketAddr;
use std::task::{Context, Poll};
use std::vec;

use http::uri::Scheme;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::connect::dns::Name;
use hyper_util::rt::TokioExecutor;
use tower_service::Service;

use crate::Tls;
use crate::connecting::{ConnectsInTime, configured};

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

/// Sends `request` over a connection of its own, to one of `addresses`, which its caller has vetted, so nothing else
/// is connected to, and no request uses another's connection, which may have been made to an address this one's
/// caller wouldn't have. The connector connects to a host that's an address without asking what it comes to, and
/// `addresses` is then that address alone. The answer's body comes as it's sent, over the connection, which ends with
/// it.
pub async fn send(
    tls: &Tls,
    addresses: Vec<SocketAddr>,
    request: Request<Full<Bytes>>,
) -> Result<Response<Incoming>, Box<dyn Error + Send + Sync>> {
    let http = configured(HttpConnector::new_with_resolver(Vetted(addresses)));
    let mut client = Client::builder(TokioExecutor::new());
    client.pool_max_idle_per_host(0);
    if request.uri().scheme() == Some(&Scheme::HTTP) {
        return Ok(client.build(ConnectsInTime(http)).request(request).await?);
    }
    let https = HttpsConnectorBuilder::new().with_tls_config(tls.config().await?).https_only().enable_http1();
    Ok(client.build(ConnectsInTime(https.wrap_connector(http))).request(request).await?)
}
