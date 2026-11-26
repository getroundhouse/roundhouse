use http::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use http::{Method, Request, Response};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use protocol::client::tool_server_checks::{NoToolList, ToolServerUnreached};
use protocol::model::ToolServerUrl;
use protocol::worker::ServerHeader;
use tool_server_http::{Names, Redirected, Server, Tls, send};

use super::unreached;
use crate::ToolSignIn;
use crate::messages::Message;

/// The calls made to one server, at the URL the person typed, which is any their agents' calls would go to: over `http`
/// too, and on a private network or not. So they aren't kept to what [`NotFetched`](crate::NotFetched) leaves, which is
/// of addresses a server names, and none is fetched here.
pub(super) struct Calls<'a> {
    names: &'a Names,
    tls: &'a Tls,
    server: Server,
}

impl ToolSignIn {
    /// The calls to `server`, each with `header`. `None` for a URL or a header HTTP can't be sent with, as a worker
    /// finds them, which the hub lets nobody give.
    pub(super) fn calls_to(&self, server: &ToolServerUrl, header: Option<&ServerHeader>) -> Option<Calls<'_>> {
        Some(Calls { names: &self.names, tls: &self.tls, server: Server::new(server, header)? })
    }
}

impl Calls<'_> {
    /// Whether they say who's asking, with a key.
    pub(super) fn keyed(&self) -> bool {
        self.server.keyed()
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

    /// The server's answer, following a redirect as [`Server::send`] says.
    async fn send(
        &self,
        method: &Method,
        headers: &[(HeaderName, HeaderValue)],
        body: Bytes,
    ) -> Result<Response<Incoming>, NoToolList> {
        let user_agent = (USER_AGENT, HeaderValue::from_static("roundhouse"));
        let headers: HeaderMap = [user_agent].into_iter().chain(headers.iter().cloned()).collect();
        let answered = self.server.send(method, &headers, &body, |request| self.send_to(request)).await?;
        answered.map_err(|Redirected| NoToolList::Redirected)
    }

    async fn send_to(&self, request: Request<Full<Bytes>>) -> Result<Response<Incoming>, NoToolList> {
        let addresses = self.names.at(request.uri()).await;
        let no_such_name = NoToolList::Unreached { why: Some(ToolServerUnreached::NoSuchName) };
        let addresses = addresses.ok().filter(|found| !found.is_empty()).ok_or(no_such_name)?;
        send(self.tls, addresses, request).await.map_err(|e| unreached(&*e))
    }
}
