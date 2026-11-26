//! Working out what a sign-in to a tool server is to, and keeping it once the person has approved it. The hub asks the
//! server and its authorization server between the two, never under the files' lock, so each checks what it needs.

use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    ToolServer, ToolServerAuth, ToolServerId, ToolServerName, ToolServerUrl, ToolSignInId, ToolSignInStatus,
};
use protocol::worker::TokenNumber;
use tool_sign_in::{Registration, SignedIn};
use tracing::warn;

use super::kept::KeptSignIn;
use super::secret::ToolServerSecret;
use super::servers::gone;
use super::signing_in::{Before, To};
use super::{SigningIn, ToolServerChange, ToolServers};
use crate::error::internal;
use crate::login_files::{Locked, LoginFile, Stored};
use crate::store::AddingToolServer;

impl ToolServers {
    /// What signing in `to` would sign in to, refused as `ToolSignInStart` says.
    pub(crate) fn signing_in(&self, to: ToolSignInTo) -> Result<SigningIn, ApiError> {
        let files = self.files.lock();
        match to {
            ToolSignInTo::New { id, name, url } => {
                let (name, url) = (ToolServerName::typed(&name)?, ToolServerUrl::typed(&url)?);
                self.free_for_new(&id, &name)?;
                Ok(SigningIn { to: To::New { id, name }, url, before: None })
            }
            ToolSignInTo::Again { id } => {
                let (server, file) = self.store.tool_server(&id)?.ok_or_else(|| gone(&id))?;
                let (ToolServerAuth::SignIn { .. }, Some(file)) = (server.auth, file) else {
                    return Err(ApiError::new(ErrorCode::InvalidRequest, "that MCP server has no sign-in"));
                };
                let before = kept_in(&files, &id, &file).map(Before::of);
                Ok(SigningIn { to: To::Again { id, file }, url: server.url, before })
            }
            ToolSignInTo::Edit { id, name, url } => {
                let (server, file) = self.store.tool_server(&id)?.ok_or_else(|| gone(&id))?;
                let changes = self.changes(&id, name, url)?;
                let url = changes.url_of(&server).clone();
                let before = file.as_ref().and_then(|file| kept_in(&files, &id, file)).map(Before::of);
                // What another server was asked to allow is nothing to this one. Roundhouse is the same to their
                // service.
                let before = before.map(|before| if changes.move_it(&server) { before.moved() } else { before });
                Ok(SigningIn { to: To::Edit { id, changes, at: server.url.place(), file }, url, before })
            }
            // Not `Unsupported`, which says the hub signs in to no server at all.
            ToolSignInTo::Unknown => {
                Err(ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know what that would sign in to"))
            }
        }
    }

    /// Keeps what the person `signed_in`, as the sign-in `made_by`, to what it was `to`, given at `given_at_ms`, and
    /// answers with the server's record. Refused with `Conflict` when another server has taken the name meanwhile, or
    /// the server's ID, and when the server there was has gone or is another, and then nothing is kept. The tokens are
    /// on disk before the record says it's signed in to.
    pub(crate) fn keep_sign_in(
        &self,
        made_by: ToolSignInId,
        to: &SigningIn,
        signed_in: SignedIn,
        given_at_ms: i64,
    ) -> Result<ToolServerChange, ApiError> {
        self.changing(|files| self.keep_sign_in_to(files, made_by, to, signed_in, given_at_ms))
    }

    fn keep_sign_in_to(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        made_by: ToolSignInId,
        to: &SigningIn,
        signed_in: SignedIn,
        given_at_ms: i64,
    ) -> Result<ToolServerChange, ApiError> {
        match &to.to {
            To::New { id, name } => {
                self.free_for_new(id, name)?;
                let kept = KeptSignIn::new(made_by, signed_in, given_at_ms, TokenNumber::FIRST, None);
                let auth = ToolServerAuth::SignIn { status: kept.status() };
                let secret = ToolServerSecret::SignIn(Box::new(kept));
                let server = files.connect(&secret, |file| {
                    let adding = AddingToolServer { id, name, url: &to.url, auth: &auth, secret: Some(file) };
                    Ok((self.store.add_tool_server(&adding)?, None))
                })?;
                Ok(ToolServerChange { server, changed: true })
            }
            To::Again { id, file } => {
                // One removed, and added again under its ID, has another file.
                if self.store.tool_server(id)?.is_none_or(|(_, holds)| holds.as_ref() != Some(file)) {
                    return Err(changed_meanwhile());
                }
                let before = kept_in(files, id, file);
                let number = KeptSignIn::next_number(before.as_ref());
                let kept = KeptSignIn::new(made_by, signed_in, given_at_ms, number, before.as_ref());
                self.keep(files, id, file, kept)
            }
            To::Edit { id, changes, at, file } => {
                let there = self.store.tool_server(id)?;
                let (server, holds) = there
                    .filter(|(server, holds)| holds == file && server.url.place() == *at)
                    .ok_or_else(changed_meanwhile)?;
                if let Some(name) = &changes.name {
                    self.free(name, id)?;
                }
                let before = holds.as_ref().and_then(|file| kept_in(files, id, file));
                // On from what another server's were too, so its worker's word of one of those is never of one of
                // these.
                let number = KeptSignIn::next_number(before.as_ref());
                let before = before.filter(|_| !changes.move_it(&server));
                let kept = KeptSignIn::new(made_by, signed_in, given_at_ms, number, before.as_ref());
                let auth = ToolServerAuth::SignIn { status: kept.status() };
                let secret = ToolServerSecret::SignIn(Box::new(kept));
                self.keep_secret(files, (server, holds), changes, &auth, &secret)
            }
        }
    }

    /// Forgets `registration`, which the authorization server of what's signed in `to` has said it doesn't know, so that
    /// signing in again registers again. The sign-in there is, whatever was made with it, stays until it's refused.
    pub(crate) fn forget_registration(&self, to: &SigningIn, registration: &Registration) -> Result<(), ApiError> {
        let (Some(id), Some(file)) = (to.to.there(), to.to.file()) else { return Ok(()) };
        self.changing(|files| {
            let holds = self.store.tool_server(id)?.is_some_and(|(_, holds)| holds.as_ref() == Some(file));
            let kept = if holds { kept_in(files, id, file) } else { None };
            if let Some(kept) = kept.filter(|kept| kept.registration.as_ref() == Some(registration)) {
                self.keep(files, id, file, kept.unregistered())?;
            }
            Ok(())
        })
    }

    /// The server signed in to by the sign-in `id`, if it was the last to, for finishing it again.
    pub(crate) fn signed_in_by(&self, id: &ToolSignInId) -> Result<Option<ToolServer>, ApiError> {
        let files = self.files.lock();
        for (server, file) in self.signed_in()? {
            if kept_in(&files, &server.id, &file).is_some_and(|kept| kept.made_by == *id) {
                return Ok(Some(server));
            }
        }
        Ok(None)
    }

    /// Writes `kept` in place of what `file`, the server `id`'s, held, and then has the server's record say how it
    /// stands, so a hub that stops between the two leaves a record that [`ToolServers::tidy`] mends.
    pub(super) fn keep(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        id: &ToolServerId,
        file: &LoginFile,
        kept: KeptSignIn,
    ) -> Result<ToolServerChange, ApiError> {
        let status = kept.status();
        files
            .replace(file, &ToolServerSecret::SignIn(Box::new(kept)))
            .map_err(|e| internal(format!("couldn't keep the sign-in: {e}")))?;
        self.say(id, status)
    }

    /// Has the record of the server `id` say its sign-in's `status`.
    pub(super) fn say(&self, id: &ToolServerId, status: ToolSignInStatus) -> Result<ToolServerChange, ApiError> {
        let auth = ToolServerAuth::SignIn { status };
        if let Some(server) = self.store.set_tool_server_auth(id, &auth)? {
            return Ok(ToolServerChange { server, changed: true });
        }
        let (server, _) = self.store.tool_server(id)?.ok_or_else(|| gone(id))?;
        Ok(ToolServerChange { server, changed: false })
    }

    /// Each server that's signed in to, and its file.
    pub(super) fn signed_in(&self) -> Result<Vec<(ToolServer, LoginFile)>, ApiError> {
        let servers = self.store.tool_servers_with_secrets()?.into_iter();
        let signed_in = servers.filter(|(server, _)| matches!(server.auth, ToolServerAuth::SignIn { .. }));
        Ok(signed_in.filter_map(|(server, file)| Some((server, file?))).collect())
    }

    /// Has each record say what its sign-in's file does, where a hub stopped between the two. A status only a newer
    /// build knows is left as it is.
    pub(super) fn say_as_kept(&self, files: &Locked<'_, ToolServerSecret>) -> Result<(), ApiError> {
        for (server, file) in self.signed_in()? {
            let ToolServerAuth::SignIn {
                status: ToolSignInStatus::Works | ToolSignInStatus::NeedsSignIn | ToolSignInStatus::NeedsMoreAccess,
            } = server.auth
            else {
                continue;
            };
            if let Some(kept) = kept_in(files, &server.id, &file) {
                self.say(&server.id, kept.status())?;
            }
        }
        Ok(())
    }

    /// Refuses a new server of `id`, called `name`, with `Conflict` when the hub has one of that ID, or of a name agents
    /// would call the same.
    fn free_for_new(&self, id: &ToolServerId, name: &ToolServerName) -> Result<(), ApiError> {
        if self.store.tool_server(id)?.is_some() {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub has that MCP server already"));
        }
        self.free(name, id)
    }
}

fn changed_meanwhile() -> ApiError {
    let message = "the MCP server was removed, or changed to another, while it was being signed in to";
    ApiError::new(ErrorCode::Conflict, message)
}

/// The sign-in `file`, the server `id`'s, holds. `None` where it holds none this build reads, and where it can't be read,
/// which is said, so one server's file stops nothing of another's.
pub(super) fn kept_in(files: &Locked<'_, ToolServerSecret>, id: &ToolServerId, file: &LoginFile) -> Option<KeptSignIn> {
    match files.read(file) {
        Ok(Stored::Secret(ToolServerSecret::SignIn(kept))) => Some(*kept),
        Ok(Stored::Secret(ToolServerSecret::Key(_)) | Stored::Newer | Stored::Nothing) => None,
        Err(e) => {
            warn!(name: "tool_sign_in.read_failed", server_id = %id, error = %e, "Couldn't read a tool server's sign-in");
            None
        }
    }
}
