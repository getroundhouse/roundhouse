use super::private::{Lookup, NotPrivate, PrivateAddresses, PrivateOnly};
use super::{Failure, Sent};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::http::uri::Scheme;
use hyper::{Request, Response, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::error::Error;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::OnceCell;
use tower_service::Service;
use tracing::warn;

/// Finding a server, connecting to it and agreeing on TLS, which a server that's there does in well under a second, so
/// this is only reached by one that isn't, or is stuck. A call has no limit here once it's sent, since a tool can take
/// long, and the agent's CLI has a limit of its own for it, and
/// ends the call by closing its connection.
pub(super) const CONNECT_WITHIN: Duration = Duration::from_secs(15);

/// How long a connection with nothing on it, such as a stream of events, goes before the system asks whether the other
/// end is still there, and then how often and how many times, so one that went without a word ends in a minute or two.
const KEEPALIVE_AFTER: Duration = Duration::from_secs(60);
const KEEPALIVE_EVERY: Duration = Duration::from_secs(15);
const KEEPALIVE_TIMES: u32 = 4;

/// What's kept of a server's connections between calls: enough for an agent's next call to go at once, and few, since
/// each holds a descriptor.
const IDLE_FOR: Duration = Duration::from_secs(30);
const IDLE_PER_SERVER: usize = 2;

type HttpClient = Client<ConnectsInTime<HttpsConnector<HttpConnector>>, Full<Bytes>>;
type PrivateClient = Client<ConnectsInTime<PrivateOnly>, Full<Bytes>>;

/// The clients that reach the hub's tool servers. One is for a request that carries a sign-in's access token over plain
/// `http`, and connects only to private addresses, with connections of its own, so none made for a key or for nothing
/// is used for a token. The other is for every other request, over `https` or `http`, made the first time one is
/// asked, since reading the system's certificates takes a moment, and a workspace may have no call to make.
pub(super) struct Upstream {
    any: OnceCell<HttpClient>,
    private: PrivateClient,
}

impl Default for Upstream {
    fn default() -> Self {
        Self::looking_up(Lookup::default())
    }
}

impl Upstream {
    /// One whose client for private addresses looks names up with `lookup`.
    pub(super) fn looking_up(lookup: Lookup) -> Self {
        let private = PrivateOnly(configured(HttpConnector::new_with_resolver(PrivateAddresses(lookup))));
        Self { any: OnceCell::new(), private: pooled(private) }
    }

    /// Sends a request, whose URL is whole and which carries `sent`, and returns the answer once its headers have come.
    pub(super) async fn send(&self, request: Request<Full<Bytes>>, sent: Sent) -> Result<Response<Incoming>, Failure> {
        let answered = if sent == Sent::AccessToken && request.uri().scheme() != Some(&Scheme::HTTPS) {
            self.private.request(request).await
        } else {
            let client = self.any.get_or_try_init(made).await.map_err(|e| {
                warn!(name: "tool_server.certificates_unread", error = %e, "Couldn't read the system's certificates");
                Failure::NoCertificates
            })?;
            client.request(request).await
        };
        answered.map_err(|e| {
            // Its caller logs it, with which server it is.
            if caused_by_not_private(&e) {
                return Failure::NotPrivate;
            }
            warn!(name: "tool_server.unanswered", error = with_causes(&e), "A tool server didn't answer");
            if e.is_connect() { Failure::Unreachable } else { Failure::Lost }
        })
    }
}

async fn made() -> io::Result<HttpClient> {
    tokio::task::spawn_blocking(client).await.map_err(io::Error::other)?
}

fn client() -> io::Result<HttpClient> {
    let mut http = configured(HttpConnector::new());
    http.enforce_http(false);
    let https = HttpsConnectorBuilder::new()
        .with_provider_and_native_roots(rustls::crypto::ring::default_provider())?
        .https_or_http()
        .enable_http1()
        .wrap_connector(http);
    Ok(pooled(https))
}

/// `http` as every connection to a tool server is made.
fn configured<R>(mut http: HttpConnector<R>) -> HttpConnector<R> {
    http.set_nodelay(true);
    http.set_keepalive(Some(KEEPALIVE_AFTER));
    http.set_keepalive_interval(Some(KEEPALIVE_EVERY));
    http.set_keepalive_retries(Some(KEEPALIVE_TIMES));
    http
}

/// A client that connects with `connector` in time, and keeps connections as every client to tool servers does.
fn pooled<C>(connector: C) -> Client<ConnectsInTime<C>, Full<Bytes>>
where
    C: Service<Uri> + Clone + Send + Sync + 'static,
    ConnectsInTime<C>: hyper_util::client::legacy::connect::Connect,
{
    let mut client = Client::builder(TokioExecutor::new());
    client.pool_timer(TokioTimer::new()).pool_idle_timeout(IDLE_FOR).pool_max_idle_per_host(IDLE_PER_SERVER);
    client.build(ConnectsInTime(connector))
}

/// Whether `e` came of a request that carries a sign-in going nowhere private.
fn caused_by_not_private(e: &(dyn Error + 'static)) -> bool {
    let mut cause = Some(e);
    while let Some(error) = cause {
        if error.is::<NotPrivate>() {
            return true;
        }
        cause = error.source();
    }
    false
}

/// An error with what caused it, none of which has the URL's query or a header: hyper says what it was doing, and
/// rustls and the system what went wrong.
fn with_causes(e: &dyn Error) -> String {
    let mut said = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        said.push_str(": ");
        said.push_str(&cause.to_string());
        source = cause.source();
    }
    said
}

/// A connector that gives up after [`CONNECT_WITHIN`], TLS and all, where hyper-util's own limit is for TCP alone.
#[derive(Clone)]
struct ConnectsInTime<C>(C);

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
