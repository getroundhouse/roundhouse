use super::{Failure, Sent};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::http::uri::Scheme;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::{self, Client};
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::OnceCell;
use tool_server_http::{Connector, Names, Tls, is_private};
use tracing::warn;

/// What's kept of a server's connections between calls: enough for an agent's next call to go at once, and few, since
/// each holds a descriptor.
const IDLE_FOR: Duration = Duration::from_secs(30);
const IDLE_PER_SERVER: usize = 2;

/// What reaches the hub's tool servers. A request that carries a sign-in's access token over plain `http` goes only to
/// a private address, as [`is_private`] says, so nobody on the internet between can read the token, over a connection
/// of its own, so none made for a key or for nothing, to wherever its name came to then, is used for a token. The
/// address is checked as the request is sent, rather than once, since a name can come to another address by then, and
/// on another machine, such as a laptop on another network, to another again. Every other request, over `https` or
/// `http`, goes through a client that keeps connections, made the first time one is asked, since reading the system's
/// certificates takes a moment, and a workspace may have no call to make.
pub(super) struct Upstream {
    names: Names,
    tls: Tls,
    any: OnceCell<Client<Connector, Full<Bytes>>>,
}

impl Default for Upstream {
    fn default() -> Self {
        Self::looking_up(Names::System)
    }
}

impl Upstream {
    /// One that looks up the names of servers a sign-in goes to over `http` with `names`.
    pub(super) fn looking_up(names: Names) -> Self {
        Self { names, tls: Tls::default(), any: OnceCell::new() }
    }

    /// Sends a request, whose URL is whole and which carries `sent`, and returns the answer once its headers have come.
    pub(super) async fn send(&self, request: Request<Full<Bytes>>, sent: Sent) -> Result<Response<Incoming>, Failure> {
        let answered = if sent == Sent::AccessToken && request.uri().scheme() != Some(&Scheme::HTTPS) {
            let addresses = self.private(request.uri()).await?;
            tool_server_http::send(&self.tls, addresses, request).await
        } else {
            let client = self.any.get_or_try_init(|| async { self.tls.connector().await.map(pooled) }).await;
            let client = client.map_err(|e| {
                warn!(name: "tool_server.certificates_unread", error = %e, "Couldn't read the system's certificates");
                Failure::NoCertificates
            })?;
            client.request(request).await.map_err(Into::into)
        };
        answered.map_err(|e| unanswered(&*e))
    }

    /// The private addresses `uri`'s host comes to, and none of its others.
    async fn private(&self, uri: &Uri) -> Result<Vec<SocketAddr>, Failure> {
        let found = self.names.at(uri).await.map_err(|e| unanswered(&e))?;
        let private: Vec<_> = found.into_iter().filter(|address| is_private(address.ip())).collect();
        // Its caller logs it, with which server it is.
        if private.is_empty() { Err(Failure::NotPrivate) } else { Ok(private) }
    }
}

/// A client that keeps connections as every client to tool servers does.
fn pooled(connector: Connector) -> Client<Connector, Full<Bytes>> {
    let mut client = Client::builder(TokioExecutor::new());
    client.pool_timer(TokioTimer::new()).pool_idle_timeout(IDLE_FOR).pool_max_idle_per_host(IDLE_PER_SERVER);
    client.build(connector)
}

/// What it came to that a request wasn't answered, for `e`.
fn unanswered(e: &(dyn Error + 'static)) -> Failure {
    warn!(name: "tool_server.unanswered", error = with_causes(e), "A tool server didn't answer");
    let connecting = e.downcast_ref::<legacy::Error>().is_none_or(legacy::Error::is_connect);
    if connecting { Failure::Unreachable } else { Failure::Lost }
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
