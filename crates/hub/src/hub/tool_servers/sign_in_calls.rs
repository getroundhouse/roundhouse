use std::sync::Arc;

use protocol::client::tool_sign_ins::{
    NoToolSignIn, ToolSignInBegun, ToolSignInFinish, ToolSignInPage, ToolSignInStart,
};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServer, ToolSignInId};
use tool_sign_in::{Failure, Found, NoWay, Pending, RedirectUri, Returned, ServerUrl, Started, Way, shown};
use tracing::info;

use super::super::{Hub, off_thread};
use super::waiting::Stage;
use crate::store::now_ms;
use crate::tool_servers::SigningIn;

impl Hub {
    /// Starts a sign-in to a tool server, as `ToolSignInStart` says. It waits from before its server is asked how to sign
    /// in to it, so one cancelled meanwhile has no page, and one sent again meanwhile is answered with the same one.
    pub(crate) async fn tool_sign_in_start(&self, asked: ToolSignInStart) -> Result<ToolSignInBegun, ApiError> {
        let ToolSignInStart { id, to, redirect } = asked;
        let sign_in = if let Some(sign_in) = self.tool_sign_ins.waiting.get(&id, now_ms()) {
            sign_in
        } else {
            let servers = Arc::clone(&self.tool_servers);
            let to = off_thread(move || servers.signing_in(to)).await?;
            self.tool_sign_ins.waiting.begin(&id, to, now_ms())?
        };
        let mut stage = sign_in.stage.lock().await;
        if let Stage::OnPage { page, .. } = &*stage {
            return Ok(ToolSignInBegun::Page { page: page.clone() });
        }
        let found = self.find_sign_in(&sign_in.to, &redirect).await;
        if !self.tool_sign_ins.waiting.holds(&id, &sign_in) {
            return Err(ApiError::new(ErrorCode::Conflict, "the sign-in was cancelled"));
        }
        match found {
            Ok(Begun::Waits { page, pending }) => {
                *stage = Stage::OnPage { page: page.clone(), pending };
                Ok(ToolSignInBegun::Page { page })
            }
            Ok(Begun::Not(why)) => {
                self.tool_sign_ins.waiting.end(&id, &sign_in);
                Ok(why)
            }
            Err(e) => {
                self.tool_sign_ins.waiting.end(&id, &sign_in);
                Err(e)
            }
        }
    }

    /// Finishes a sign-in to a tool server, as `ToolSignInFinish` says, and tells every client of the server's record.
    pub(crate) async fn tool_sign_in_finish(&self, asked: ToolSignInFinish) -> Result<ToolServer, ApiError> {
        let ToolSignInFinish { id, returned } = asked;
        let Some(sign_in) = self.tool_sign_ins.waiting.get(&id, now_ms()) else {
            return self.finished_before(id).await;
        };
        // Held until it's kept, so one sent again meanwhile is answered with what this comes to.
        let stage = sign_in.stage.lock().await;
        let Stage::OnPage { pending, .. } = &*stage else { return Err(no_sign_in()) };
        if !self.tool_sign_ins.waiting.holds(&id, &sign_in) {
            return self.finished_before(id).await;
        }
        let signed_in = match self.tool_sign_ins.client.finish(pending, Returned::of_query(returned.as_str())).await {
            Ok(signed_in) => signed_in,
            // Nothing of this sign-in, so it still waits.
            Err(e @ Failure::WrongState) => return Err(e.into()),
            Err(e) => {
                self.tool_sign_ins.waiting.end(&id, &sign_in);
                return Err(e.into());
            }
        };
        let given_at_ms = now_ms();
        if !self.tool_sign_ins.waiting.keep(&id, &sign_in) {
            return Err(no_sign_in());
        }
        let (servers, made_by, to) = (Arc::clone(&self.tool_servers), id.clone(), Arc::clone(&sign_in));
        let kept = off_thread(move || servers.keep_sign_in(made_by, &to.to, signed_in, given_at_ms)).await;
        self.tool_sign_ins.waiting.end(&id, &sign_in);
        let server = self.announce_tool_server(kept?);
        info!(name: "tool_sign_in.signed_in", server_id = %server.id, "Signed in to a tool server");
        Ok(server)
    }

    /// Forgets a sign-in to a tool server, as `ToolSignInCancel` says.
    pub(crate) fn tool_sign_in_cancel(&self, id: &ToolSignInId) {
        self.tool_sign_ins.waiting.cancel(id);
    }

    /// The server the sign-in `id`, which doesn't wait, signed in to, when it was finished.
    async fn finished_before(&self, id: ToolSignInId) -> Result<ToolServer, ApiError> {
        let servers = Arc::clone(&self.tool_servers);
        off_thread(move || servers.signed_in_by(&id)).await?.ok_or_else(no_sign_in)
    }

    /// Asks the server how it's signed in to, and starts that, with the person sent back to `redirect`.
    async fn find_sign_in(&self, to: &SigningIn, redirect: &str) -> Result<Begun, ApiError> {
        let refused = |e: &dyn std::error::Error| ApiError::new(ErrorCode::InvalidRequest, e.to_string());
        let redirect = RedirectUri::parse(redirect).map_err(|e| refused(&e))?;
        let server = ServerUrl::parse(to.url().as_str()).map_err(|e| refused(&e))?;
        Ok(match self.tool_sign_ins.client.find(&server).await? {
            Found::SignIn(way) => {
                let Started { page, pending } = self.begin_sign_in(*way, to, redirect)?;
                Begun::Waits { page: ToolSignInPage::new(page.as_str()), pending: Box::new(pending) }
            }
            Found::NoSignIn => Begun::Not(ToolSignInBegun::NoSignIn),
            Found::NoWay(why) => Begun::Not(ToolSignInBegun::NoWay { why: no_way(why) }),
        })
    }

    /// Starts signing in by `way`, asking to be allowed what the server was signed in with before too, so nothing that
    /// worked stops working. What another authorization server was asked to allow is nothing to this one.
    fn begin_sign_in(&self, way: Way, to: &SigningIn, redirect: RedirectUri) -> Result<Started, Failure> {
        let mut scopes = way.scopes().clone();
        if let Some(before) = to.before().filter(|before| before.issuer() == way.issuer()) {
            scopes = scopes.and(before.scopes());
        }
        self.tool_sign_ins.client.begin(way, scopes, redirect)
    }
}

/// What a start's server was found to have.
enum Begun {
    Waits {
        page: ToolSignInPage,
        pending: Box<Pending>,
    },
    /// No sign-in, as the start is answered.
    Not(ToolSignInBegun),
}

/// Why a finish finds no sign-in of its ID.
fn no_sign_in() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "that sign-in to an MCP server has ended")
}

fn no_way(why: NoWay) -> NoToolSignIn {
    match why {
        NoWay::NoAuthorizationServer => NoToolSignIn::NoAuthorizationServer,
        NoWay::OnlyRegisteredClients(issuer) => {
            NoToolSignIn::OnlyRegisteredClients { authorization_server: shown(issuer.as_str()) }
        }
        NoWay::OtherResource { said } => NoToolSignIn::OtherServer { said: shown(&said) },
        NoWay::NoS256(issuer) => NoToolSignIn::NoS256 { authorization_server: shown(issuer.as_str()) },
    }
}
