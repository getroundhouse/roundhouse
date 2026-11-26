//! Keeping what access a server wants that its sign-in hasn't, for signing in to it again to ask for. It's beside the
//! sign-in, in the server's file, since what a server names is its own text, and the record, which every client has, says
//! only that it wants more. What workers are sent doesn't change, since the access token is good for the rest.

use protocol::client::ApiError;
use protocol::model::ToolScopes;
use protocol::worker::{AskForToolAccess, ToolAccess};
use tracing::{info, warn};

use super::kept::KeptSignIn;
use super::secret::ToolServerSecret;
use super::servers::gone;
use super::sign_ins::kept_in;
use super::{ToolServerChange, ToolServers};
use crate::login_files::Locked;

/// How many scopes a server can want between one sign-in and the next, and how long each can be: far more than a server
/// was seen to name, and few enough that one that names new ones at each call can't have its file and its sign-in's page
/// grow with each. *Unconfirmed.*
const MOST_WANTED: usize = 32;
const LONGEST_WANTED: usize = 200;

/// What came of a server wanting more access.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub(crate) access: ToolAccess,
    /// `None` where nothing was kept.
    pub(crate) change: Option<ToolServerChange>,
}

impl Wanted {
    fn nothing_kept(access: ToolAccess) -> Self {
        Self { access, change: None }
    }
}

impl ToolServers {
    /// Keeps what the server `asked` of wants, and answers with the server's record, which says so, as
    /// [`AskForToolAccess`] says. Nothing is kept of a token that isn't the one the hub has, since that one may be allowed
    /// more. Nor is what the sign-in has been asked again for already, as [`KeptSignIn::asked_again`] says, so a server
    /// that never takes its sign-in as enough doesn't have the person sign in without end.
    pub(crate) fn wants_access(&self, asked: &AskForToolAccess) -> Result<Wanted, ApiError> {
        self.changing(|files| self.wants_access_in(files, asked))
    }

    fn wants_access_in(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        asked: &AskForToolAccess,
    ) -> Result<Wanted, ApiError> {
        let AskForToolAccess { server: id, refused, wants } = asked;
        let (_, file) = self.store.tool_server(id)?.ok_or_else(|| gone(id))?;
        let held = file.and_then(|file| Some((kept_in(files, id, &file)?, file)));
        let Some((kept, file)) = held.filter(|(kept, _)| kept.number == *refused) else {
            return Ok(Wanted::nothing_kept(ToolAccess::OtherToken));
        };
        if wants.is_empty() || kept.wants_in_vain(wants) {
            return Ok(Wanted::nothing_kept(ToolAccess::NotAskedFor));
        }
        let all = kept.wants.clone().and(wants);
        if all == kept.wants {
            return Ok(Wanted::nothing_kept(ToolAccess::SignInAgain));
        }
        if !is_kept(&all) {
            warn!(name: "tool_sign_in.wants_too_much", server_id = %id, "A tool server wants more scopes than the hub keeps");
            return Ok(Wanted::nothing_kept(ToolAccess::NotAskedFor));
        }
        info!(name: "tool_sign_in.wants_access", server_id = %id, "A tool server wants more access than its sign-in has");
        let change = self.keep(files, id, &file, KeptSignIn { wants: all, ..kept })?;
        Ok(Wanted { access: ToolAccess::SignInAgain, change: Some(change) })
    }
}

fn is_kept(wants: &ToolScopes) -> bool {
    wants.iter().len() <= MOST_WANTED && wants.iter().all(|scope| scope.len() <= LONGEST_WANTED)
}
