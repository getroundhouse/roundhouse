use super::Failure;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
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

/// The client that reaches the hub's tool servers, made the first time one is asked, since reading the system's
/// certificates takes a moment, and a workspace may have no call to make.
#[derive(Default)]
pub(super) struct Upstream(OnceCell<HttpClient>);

impl Upstream {
    /// Sends a request, whose URL is whole, and returns the answer once its headers have come.
    pub(super) async fn send(&self, request: Request<Full<Bytes>>) -> Result<Response<Incoming>, Failure> {
        let client = self.0.get_or_try_init(made).await.map_err(|e| {
            warn!(name: "tool_server.certificates_unread", error = %e, "Couldn't read the system's certificates");
            Failure::NoCertificates
        })?;
        client.request(request).await.map_err(|e| {
            warn!(name: "tool_server.unanswered", error = with_causes(&e), "A tool server didn't answer");
            if e.is_connect() { Failure::Unreachable } else { Failure::Lost }
        })
    }
}

async fn made() -> io::Result<HttpClient> {
    tokio::task::spawn_blocking(client).await.map_err(io::Error::other)?
}

fn client() -> io::Result<HttpClient> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    http.set_nodelay(true);
    http.set_keepalive(Some(KEEPALIVE_AFTER));
    http.set_keepalive_interval(Some(KEEPALIVE_EVERY));
    http.set_keepalive_retries(Some(KEEPALIVE_TIMES));
    let https = HttpsConnectorBuilder::new()
        .with_provider_and_native_roots(rustls::crypto::ring::default_provider())?
        .https_or_http()
        .enable_http1()
        .wrap_connector(http);
    let mut client = Client::builder(TokioExecutor::new());
    client.pool_timer(TokioTimer::new()).pool_idle_timeout(IDLE_FOR).pool_max_idle_per_host(IDLE_PER_SERVER);
    Ok(client.build(ConnectsInTime(https)))
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
