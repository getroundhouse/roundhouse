use http::header::{HeaderName, HeaderValue, USER_AGENT};
use http::uri::Scheme;
use http::{Method, Request, Response, Uri};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use protocol::client::tool_server_checks::{NoToolList, ToolServerUnreached};
use protocol::model::ToolServerUrl;
use protocol::worker::ServerHeader;
use tokio::sync::OnceCell;

use super::unreached;
use crate::ToolSignIn;
use crate::fetch::{Names, Tls, send};
use crate::messages::Message;
use protocol::tool_server_http::{MOST_REDIRECTS_WITHIN, Redirected, redirected};

/// The calls made to one server, at the URL the person typed, which is any their agents' calls would go to: over `http`
/// too, and on a private network or not. So they aren't kept to what [`NotFetched`](crate::NotFetched) leaves, which is
/// of addresses a server names, and none is fetched here.
pub(super) struct Calls<'a> {
    names: &'a Names,
    tls: &'a OnceCell<Tls>,
    server: Uri,
    header: Option<(HeaderName, HeaderValue)>,
}

impl ToolSignIn {
    /// The calls to `server`, each with `header`. `None` for a URL or a header HTTP can't be sent with, as a worker
    /// finds them, which the hub lets nobody give.
    pub(super) fn calls_to(&self, server: &ToolServerUrl, header: Option<&ServerHeader>) -> Option<Calls<'_>> {
        let server: Uri = server.as_str().parse().ok()?;
        server.host()?;
        let header = header.map(sent);
        let header = match header {
            Some(header) => Some(header?),
            None => None,
        };
        Some(Calls { names: &self.names, tls: &self.tls, server, header })
    }
}

fn sent(header: &ServerHeader) -> Option<(HeaderName, HeaderValue)> {
    let name = HeaderName::from_bytes(header.name.as_str().as_bytes()).ok()?;
    let mut value = HeaderValue::from_str(header.value.as_str()).ok()?;
    value.set_sensitive(true);
    Some((name, value))
}

impl Calls<'_> {
    /// Whether they say who's asking, with a key.
    pub(super) fn keyed(&self) -> bool {
        self.header.is_some()
    }

    /// Posts `message`, with `session`'s headers, and gives the answer once its headers have come.
    pub(super) async fn post(
        &self,
        message: &Message,
        session: &[(HeaderName, HeaderValue)],
    ) -> Result<Response<Incoming>, NoToolList> {
        let every = Message::EVERY_HAS.iter().chain(message.headers);
        let every = every.map(|(name, value)| (HeaderName::from_static(name), HeaderValue::from_static(value)));
        let headers: Vec<_> = every.chain(session.iter().cloned()).collect();
        self.send(&Method::POST, &headers, message.body.to_string().into()).await
    }

    /// Ends `session`, whose headers these are.
    pub(super) async fn delete(&self, session: &[(HeaderName, HeaderValue)]) -> Result<(), NoToolList> {
        self.send(&Method::DELETE, session, Bytes::new()).await.map(drop)
    }

    /// The server's answer, following a redirect as [`redirected`] says.
    async fn send(
        &self,
        method: &Method,
        headers: &[(HeaderName, HeaderValue)],
        body: Bytes,
    ) -> Result<Response<Incoming>, NoToolList> {
        let mut to = self.server.clone();
        for _ in 0..=MOST_REDIRECTS_WITHIN {
            let response = self.send_to(&to, method, headers, body.clone()).await?;
            match redirected(&to, response.status(), response.headers()) {
                Redirected::Not => return Ok(response),
                Redirected::Within(within) => to = within,
                Redirected::Elsewhere => return Err(NoToolList::Redirected),
            }
        }
        Err(NoToolList::Redirected)
    }

    async fn send_to(
        &self,
        to: &Uri,
        method: &Method,
        headers: &[(HeaderName, HeaderValue)],
        body: Bytes,
    ) -> Result<Response<Incoming>, NoToolList> {
        // An IPv6 address's brackets are the URL's.
        let host = to.host().unwrap_or_default().trim_start_matches('[').trim_end_matches(']');
        let own_port = if to.scheme() == Some(&Scheme::HTTP) { 80 } else { 443 };
        let addresses = self.names.of(host, to.port_u16().unwrap_or(own_port)).await;
        let no_such_name = NoToolList::Unreached { why: Some(ToolServerUnreached::NoSuchName) };
        let addresses = addresses.ok().filter(|found| !found.is_empty()).ok_or(no_such_name)?;
        let mut request = Request::new(Full::new(body));
        *request.method_mut() = method.clone();
        *request.uri_mut() = to.clone();
        let user_agent = (USER_AGENT, HeaderValue::from_static("roundhouse"));
        // The server's own over any of its name.
        for (name, value) in [user_agent].iter().chain(headers).chain(&self.header) {
            request.headers_mut().insert(name, value.clone());
        }
        send(self.tls, addresses, request).await.map_err(|e| unreached(&*e))
    }
}
