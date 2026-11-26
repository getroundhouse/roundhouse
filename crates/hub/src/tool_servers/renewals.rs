//! Which sign-ins to renew before their access tokens end, by the wall clock, and keeping what renewing one came to. The
//! hub asks the authorization server between the two, never under the files' lock.

use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::{ToolServer, ToolServerId};
use tool_sign_in::{RefreshToken, Renewable, Renewal};
use tracing::{info, warn};

use super::kept::{KeptSignIn, KeptTokens, TokenNumber};
use super::secret::ToolServerSecret;
use super::sign_ins::kept_in;
use super::{ToolServerChange, ToolServers};
use crate::login_files::{Locked, LoginFile};
use crate::store::later_ms;

/// How much an access token has left when the hub renews it. Authorization servers give ones that last an hour or so, so
/// this leaves a check each minute a few tries before the token ends, and renews one that lasts as little as this at
/// each check.
pub(crate) const RENEWED_WITH_LEFT: Duration = Duration::from_mins(5);

/// A sign-in to renew, as it was found, which tells, once its authorization server has answered, whether it's that
/// sign-in still.
#[derive(Clone, Debug)]
pub(crate) struct Due {
    pub(crate) server: ToolServerId,
    pub(crate) renewable: Renewable,
    pub(crate) refresh: RefreshToken,
    pub(crate) of: Held,
}

/// Which tokens of which sign-in: the file they're in, which no other server's ever is, and which of those it has held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    file: LoginFile,
    number: TokenNumber,
}

/// What a check of the sign-ins found.
#[derive(Debug, Default)]
pub(crate) struct Checked {
    pub(crate) due: Vec<Due>,
    /// The record of each server whose sign-in the check found ended, with nothing to renew it.
    pub(crate) changed: Vec<ToolServer>,
}

impl ToolServers {
    /// Checks the sign-ins at `now_ms` by the wall clock. One whose access token has less than [`RENEWED_WITH_LEFT`]
    /// left, or has ended, is due. One nothing renews works until its access token ends, and is then gone, so it needs
    /// signing in to. One whose end nobody said is used until it's refused.
    pub(crate) fn check_sign_ins(&self, now_ms: i64) -> Result<Checked, ApiError> {
        self.changing(|files| self.check_sign_ins_in(files, now_ms))
    }

    fn check_sign_ins_in(&self, files: &Locked<'_, ToolServerSecret>, now_ms: i64) -> Result<Checked, ApiError> {
        let mut checked = Checked::default();
        for (server, file) in self.signed_in()? {
            let Some(kept) = kept_in(files, &server.id, &file) else { continue };
            let Some(KeptTokens { ends_at_ms: Some(ends_at_ms), refresh, .. }) = &kept.tokens else { continue };
            match refresh {
                Some(refresh) if *ends_at_ms < later_ms(now_ms, RENEWED_WITH_LEFT) => checked.due.push(Due {
                    server: server.id,
                    renewable: kept.renewable.clone(),
                    refresh: refresh.clone(),
                    of: Held { file, number: kept.number },
                }),
                None if *ends_at_ms <= now_ms => {
                    info!(name: "tool_sign_in.ended", server_id = %server.id, "A tool server's sign-in ended, and nothing renews it");
                    checked.changed.push(self.keep(files, &server.id, &file, kept.gone())?.server);
                }
                Some(_) | None => {}
            }
        }
        Ok(checked)
    }

    /// Whether the hub still has the tokens of `due`, so it's worth asking with.
    pub(crate) fn holds(&self, due: &Due) -> Result<bool, ApiError> {
        Ok(self.held_still(&self.files.lock(), due)?.is_some())
    }

    /// Keeps what renewing `due` came to, as its authorization server answered at `answered_at_ms`, and answers with the
    /// server's record: the new tokens in its file, on disk before anything uses them, or, when it was refused for good,
    /// the last of them forgotten and then the record saying it needs signing in to. `None`, and nothing kept, when the
    /// server has been removed or signed in to again meanwhile, since that isn't the sign-in that was asked about.
    pub(crate) fn renewed(
        &self,
        due: &Due,
        renewal: Renewal,
        answered_at_ms: i64,
    ) -> Result<Option<ToolServerChange>, ApiError> {
        self.changing(|files| self.renewed_in(files, due, renewal, answered_at_ms))
    }

    fn renewed_in(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        due: &Due,
        renewal: Renewal,
        answered_at_ms: i64,
    ) -> Result<Option<ToolServerChange>, ApiError> {
        let server_id = &due.server;
        let Some(kept) = self.held_still(files, due)? else {
            info!(name: "tool_sign_in.renewal_let_go", %server_id, "Let go of what renewing a tool server's sign-in came to, as it's another now");
            return Ok(None);
        };
        let kept = match renewal {
            Renewal::Renewed(granted) => kept.renewed(granted, answered_at_ms),
            Renewal::SignInAgain(refused) => {
                warn!(name: "tool_sign_in.refused", %server_id, %refused, "A tool server's sign-in won't be renewed, so it needs signing in to");
                kept.gone()
            }
        };
        self.keep(files, server_id, &due.of.file, kept).map(Some)
    }

    /// The sign-in of `due`, while its server has it with those tokens.
    fn held_still(&self, files: &Locked<'_, ToolServerSecret>, due: &Due) -> Result<Option<KeptSignIn>, ApiError> {
        let holds = self.store.tool_server(&due.server)?.and_then(|(_, file)| file);
        let kept = holds.filter(|file| *file == due.of.file).and_then(|file| kept_in(files, &due.server, &file));
        Ok(kept.filter(|kept| kept.number == due.of.number && kept.tokens.is_some()))
    }
}
