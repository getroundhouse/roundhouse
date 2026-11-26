use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use http::Uri;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use tower_service::Service;

/// Finding a server, connecting to it and agreeing on TLS, which a server that's there does in well under a second, so
/// this is only reached by one that isn't, or is stuck. A call has no limit here once it's sent, since a tool can take
/// long, and an agent's CLI has a limit of its own for it, and ends the call by closing its connection.
pub const CONNECT_WITHIN: Duration = Duration::from_secs(15);

/// How long a connection with nothing on it, such as a stream of events, goes before the system asks whether the other
/// end is still there, and then how often and how many times, so one that went without a word ends in a minute or two.
const KEEPALIVE_AFTER: Duration = Duration::from_secs(60);
const KEEPALIVE_EVERY: Duration = Duration::from_secs(15);
const KEEPALIVE_TIMES: u32 = 4;

/// What connects to any server, over TLS where its URL says `https`, as [`Tls::connector`](crate::Tls::connector)
/// makes it.
pub type Connector = ConnectsInTime<HttpsConnector<HttpConnector>>;

/// `http` as every connection to a server is made, to whatever its URL's scheme says.
pub(crate) fn configured<R>(mut http: HttpConnector<R>) -> HttpConnector<R> {
    http.enforce_http(false);
    http.set_nodelay(true);
    http.set_keepalive(Some(KEEPALIVE_AFTER));
    http.set_keepalive_interval(Some(KEEPALIVE_EVERY));
    http.set_keepalive_retries(Some(KEEPALIVE_TIMES));
    http
}

/// A connector that gives up after [`CONNECT_WITHIN`], TLS and all, where hyper-util's own limit is for TCP alone.
#[derive(Clone)]
pub struct ConnectsInTime<C>(pub(crate) C);

impl<C> Service<Uri> for ConnectsInTime<C>
where
    C: Service<Uri>,
    C::Response: 'static,
    C::Error: Into<Box<dyn Error + Send + Sync>>,
    C::Future: Send + 'static,
{
    type Response = C::Response;
    type Error = Box<dyn Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<C::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, to: Uri) -> Self::Future {
        let connecting = self.0.call(to);
        Box::pin(async move { tokio::time::timeout(CONNECT_WITHIN, connecting).await?.map_err(Into::into) })
    }
}
