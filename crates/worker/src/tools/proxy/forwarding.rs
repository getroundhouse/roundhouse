use super::connections::CutOff;
use super::server::Server;
use super::{Failure, PassedOn, Proxy, Target, failure, headers};
use crate::asks::Asks;
use crate::tools::http::AnswerBody;
use http_body_util::{Either, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::HeaderMap;
use hyper::{Method, Request, Response, StatusCode, Uri};
use protocol::model::{SessionId, ToolServerId};
use protocol::tool_server_http::{MOST_REDIRECTS_WITHIN, Redirected, access_wanted, redirected};
use protocol::worker::{AskForToolAccess, RenewToolServer, ToolAccess};
use tracing::{info, warn};

/// What MCP's streamable HTTP asks a server: a message, its stream of events, and to end a session.
pub(in super::super) const METHODS: [Method; 3] = [Method::POST, Method::GET, Method::DELETE];

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
        let Some(server) = self.server(id) else { return failure::not_set(&asked.body) };
        let let_in = match self.connections.lets_in(&server.id, &server.place, &asked) {
            Ok(let_in) => let_in,
            Err(CutOff { first }) => {
                // Not each time, since a CLI can go on asking, as Claude does of a task each second.
                if first {
                    let session_id = asked.session_id;
                    info!(
                        name: "tool_server.cut_off",
                        server_id = %server.id,
                        %session_id,
                        "An agent is cut off from a tool server that was moved"
                    );
                }
                return Failure::Moved.answer(server.label.as_str(), &asked.body);
            }
        };
        match self.answered(&server, &asked).await {
            Ok(response) => {
                let said = (response.status(), response.headers());
                self.connections.answered(&server.id, &server.place, &asked, let_in, said);
                passed_on(response, |body| PassedOn::new(body, &asked, self.gone(&server)))
            }
            Err(failure) => {
                warn!(name: "tool_server.request_failed", server_id = %server.id, ?failure, "A request to a tool server failed");
                failure.answer(server.label.as_str(), &asked.body)
            }
        }
    }

    /// The server's answer, unless it refuses. One that refuses its access token is asked once more, if the hub has another
    /// for it.
    async fn answered(&self, server: &Server, asked: &Asked<'_>) -> Result<Response<Incoming>, Failure> {
        let target = server.target.as_ref().map_err(|unusable| *unusable)?;
        let response = self.ask(target, asked).await?;
        if response.status() == StatusCode::UNAUTHORIZED
            && let Some(renewed) = renewed(server, target, asked).await?
        {
            let response = self.ask(&renewed, asked).await?;
            return unless_refused(server, &renewed, response, asked).await;
        }
        unless_refused(server, target, response, asked).await
    }

    /// The server's answer, following a redirect as [`redirected`] says.
    async fn ask(&self, target: &Target, asked: &Asked<'_>) -> Result<Response<Incoming>, Failure> {
        let mut to = target.uri.clone();
        for _ in 0..=MOST_REDIRECTS_WITHIN {
            let response = self.upstream.send(request(target, &to, asked), target.sent()).await?;
            match redirected(&to, response.status(), response.headers()) {
                Redirected::Not => return Ok(response),
                Redirected::Within(within) => to = within,
                Redirected::Elsewhere => return Err(Failure::Redirected),
            }
        }
        Err(Failure::Redirected)
    }
}

/// Where `server` is now that it has refused the access token `target` has, if the hub has another for it.
async fn renewed(server: &Server, target: &Target, asked: &Asked<'_>) -> Result<Option<Target>, Failure> {
    let Some(refused) = target.token() else { return Ok(None) };
    let renewing = RenewToolServer { server: server.id.clone(), refused };
    match asked.asks.ask(asked.session_id.clone(), renewing).await {
        Ok(renewed) => {
            let renewed = Server::new(renewed);
            if renewed.place != server.place {
                return Err(Failure::Moved);
            }
            let renewed = renewed.target?;
            Ok((renewed.token() != Some(refused)).then_some(renewed))
        }
        Err(e) => {
            warn!(
                name: "tool_server.not_renewed",
                server_id = %server.id,
                error = e.message,
                "The hub gave no other access token for a tool server that refused its own"
            );
            Ok(None)
        }
    }
}

/// What `server`, at `target`, answered, unless that's a 401 or a 403.
async fn unless_refused(
    server: &Server,
    target: &Target,
    response: Response<Incoming>,
    asked: &Asked<'_>,
) -> Result<Response<Incoming>, Failure> {
    let status = response.status();
    if !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Ok(response);
    }
    // Only what it says of a token the hub gave is the hub's to do something about.
    if status == StatusCode::FORBIDDEN
        && let Some(refused) = target.token()
        && let Some(wants) = access_wanted(response.headers())
    {
        let wants = AskForToolAccess { server: server.id.clone(), refused, wants };
        return Err(Failure::WantsAccess(access_to(wants, asked).await));
    }
    Err(Failure::Refused { sent: target.sent(), status })
}

/// What the hub makes of a server that `wants` more access. The hub isn't told of one that doesn't say which, since
/// signing in again would ask for nothing more. One the hub couldn't be told of now it's told of at the next call.
async fn access_to(wants: AskForToolAccess, asked: &Asked<'_>) -> ToolAccess {
    if wants.wants.is_empty() {
        return ToolAccess::NotAskedFor;
    }
    let server_id = wants.server.clone();
    asked.asks.ask(asked.session_id.clone(), wants).await.unwrap_or_else(|e| {
        warn!(
            name: "tool_server.access_not_asked_for",
            %server_id,
            error = e.message,
            "The hub couldn't be told of a tool server that wants more access"
        );
        ToolAccess::SignInAgain
    })
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

fn passed_on(response: Response<Incoming>, body_of: impl FnOnce(Incoming) -> PassedOn) -> Response<AnswerBody> {
    let (parts, body) = response.into_parts();
    let mut answer = Response::new(Either::Right(body_of(body)));
    *answer.status_mut() = parts.status;
    *answer.headers_mut() = headers::to_agent(&parts.headers);
    answer
}
