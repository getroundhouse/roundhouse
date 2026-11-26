use super::server::Server;
use super::{Failure, Proxy, Target, headers, redirect};
use crate::asks::Asks;
use crate::tools::http::AnswerBody;
use http_body_util::{Either, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::HeaderMap;
use hyper::{Method, Request, Response, StatusCode, Uri};
use protocol::model::{SessionId, ToolServerId};
use protocol::worker::RenewToolServer;
use tracing::warn;

/// What MCP's streamable HTTP asks a server: a message, its stream of events, and to end a session.
pub(in super::super) const METHODS: [Method; 3] = [Method::POST, Method::GET, Method::DELETE];

/// More than a server that adds a slash to its path, or has moved once, needs, and few enough that one going in circles
/// ends soon.
const MAX_REDIRECTS: usize = 3;

/// A request of an agent's to one of the hub's servers.
pub(in super::super) struct Asked<'a> {
    pub(in super::super) method: &'a Method,
    pub(in super::super) headers: &'a HeaderMap,
    /// Whole, since it's one message, whose ID an error answers, and which a redirect has sent again.
    pub(in super::super) body: Bytes,
    /// The agent's chat, and what its worker asks its hub with.
    pub(in super::super) session_id: &'a SessionId,
    pub(in super::super) asks: &'a Asks,
}

impl Proxy {
    /// Passes a request on to its server, and the answer back as it comes, or says why there's none.
    pub(in super::super) async fn forward(&self, id: &ToolServerId, asked: Asked<'_>) -> Response<AnswerBody> {
        let Some(server) = self.server(id) else { return Failure::Removed.answer("", &asked.body) };
        match self.answered(&server, &asked).await {
            Ok(response) => passed_on(response),
            Err(failure) => {
                warn!(name: "tool_server.request_failed", server_id = %server.id, ?failure, "A request to a tool server failed");
                failure.answer(server.label.as_str(), &asked.body)
            }
        }
    }

    /// The server's answer. One that refuses its access token is asked once more, if the hub has another for it.
    async fn answered(&self, server: &Server, asked: &Asked<'_>) -> Result<Response<Incoming>, Failure> {
        let target = server.target.as_ref().map_err(|unusable| *unusable)?;
        let answered = self.ask(target, asked).await;
        let (Err(refusal @ Failure::Refused { status: StatusCode::UNAUTHORIZED, .. }), Some(refused)) =
            (&answered, target.token())
        else {
            return answered;
        };
        let renewing = RenewToolServer { server: server.id.clone(), refused };
        let renewed = match asked.asks.ask(asked.session_id.clone(), renewing).await {
            Ok(renewed) => Server::new(renewed).target?,
            Err(e) => {
                warn!(
                    name: "tool_server.not_renewed",
                    server_id = %server.id,
                    error = e.message,
                    "The hub gave no other access token for a tool server that refused its own"
                );
                return Err(*refusal);
            }
        };
        if renewed.token() == Some(refused) { Err(*refusal) } else { self.ask(&renewed, asked).await }
    }

    /// The server's answer, following a redirect that keeps the method and the body, within its origin.
    async fn ask(&self, target: &Target, asked: &Asked<'_>) -> Result<Response<Incoming>, Failure> {
        let mut to = target.uri.clone();
        for _ in 0..=MAX_REDIRECTS {
            let response = self.upstream.send(request(target, &to, asked)).await?;
            match response.status() {
                status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
                    return Err(Failure::Refused { sent: target.sent(), status });
                }
                StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => {
                    to = redirect::at_same_origin(&to, response.headers()).ok_or(Failure::Redirected)?;
                }
                status if status.is_redirection() => return Err(Failure::Redirected),
                _ => return Ok(response),
            }
        }
        Err(Failure::Redirected)
    }
}

/// What the server is sent: the agent's request without the session's token, which isn't among the headers passed on,
/// and with the server's own header, over any of its name from the agent.
fn request(target: &Target, to: &Uri, asked: &Asked<'_>) -> Request<Full<Bytes>> {
    let mut request = Request::new(Full::new(asked.body.clone()));
    *request.method_mut() = asked.method.clone();
    *request.uri_mut() = to.clone();
    *request.headers_mut() = headers::to_server(asked.headers);
    if let Some((name, value)) = &target.header {
        request.headers_mut().insert(name, value.clone());
    }
    request
}

fn passed_on(response: Response<Incoming>) -> Response<AnswerBody> {
    let (parts, body) = response.into_parts();
    let mut answer = Response::new(Either::Right(body));
    *answer.status_mut() = parts.status;
    *answer.headers_mut() = headers::to_agent(&parts.headers);
    answer
}
