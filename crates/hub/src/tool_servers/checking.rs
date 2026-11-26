//! Checking a server as it's to be added or edited: whether there's anything to ask it, and with what. It changes
//! nothing, and the hub asks the server once the servers' lock has been let go.

use protocol::client::ApiError;
use protocol::client::tool_server_checks::{ToolServerCheck, ToolServerToCheck};
use protocol::model::{ToolServerAuth, ToolServerId, ToolServerUrl};
use protocol::worker::ServerHeader;

use super::ToolServers;
use super::editing::{Becomes, Editing};
use super::secret::{ToolServerSecret, refused};
use super::sending::{Known, LeftOut, knows_by, secret};
use super::servers::{Adding, ToAdd};
use crate::login_files::Locked;

/// What there is to ask of a server that's checked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Checking {
    Nothing,
    Ask(ToAsk),
}

/// A server to ask, as workers would be sent it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToAsk {
    /// `None` for one that isn't the hub's.
    pub(crate) id: Option<ToolServerId>,
    pub(crate) url: ToolServerUrl,
    pub(crate) header: Option<ServerHeader>,
}

impl ToolServers {
    /// What there is to ask of a server to check it, refused as `ToolServerCheck` says.
    pub(crate) fn checking(&self, asked: ToolServerCheck) -> Result<Checking, ApiError> {
        self.checking_in(&self.files.lock(), asked.of)
    }

    /// What there is to ask of a server before `of` is done, under the servers' lock, `files`. A key the server has is
    /// read with where the server is, under the lock, so it's only ever asked of the server it was given for.
    pub(super) fn checking_in(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        of: ToolServerToCheck,
    ) -> Result<Checking, ApiError> {
        match of {
            ToolServerToCheck::Add(add) => Ok(match self.adding(add)? {
                ToAdd::There(_) => Checking::Nothing,
                ToAdd::New(Adding { url, auth, secret, .. }) => {
                    Checking::of(None, url, knows_by(&auth, || secret.ok_or(LeftOut::Gone)))
                }
            }),
            ToolServerToCheck::Edit(edit) => {
                let Editing { server, file, changes, becomes } = self.editing(edit)?;
                let kept = || secret(files, file.as_ref());
                let url = changes.url_of(&server).clone();
                let as_it_was = url == server.url;
                let known = match becomes {
                    Becomes::AsItIs if as_it_was => return Ok(Checking::Nothing),
                    Becomes::AsItIs => knows_by(&server.auth, kept),
                    Becomes::KeptKey { header } => {
                        let auth = ToolServerAuth::Key { header };
                        if as_it_was && auth == server.auth {
                            return Ok(Checking::Nothing);
                        }
                        knows_by(&auth, kept)
                    }
                    Becomes::Key { header, key } => {
                        knows_by(&ToolServerAuth::Key { header }, || Ok(ToolServerSecret::Key(key)))
                    }
                    Becomes::Neither if as_it_was && server.auth == ToolServerAuth::Neither => {
                        return Ok(Checking::Nothing);
                    }
                    Becomes::Neither => knows_by(&ToolServerAuth::Neither, kept),
                };
                Ok(Checking::of(Some(server.id), url, known))
            }
            // Not `Unsupported`, which says the hub checks no server at all.
            ToolServerToCheck::Unknown => Err(refused("this hub doesn't know what that would check")),
        }
    }
}

impl Checking {
    /// Nothing is asked of a server that's signed in to, whose sign-in says whether it works, nor of one whose key this
    /// build can't read, which workers aren't sent either.
    fn of(id: Option<ToolServerId>, url: ToolServerUrl, known: Result<Known, LeftOut>) -> Self {
        let header = match known {
            Ok(Known::Not) => None,
            Ok(Known::By(header)) => Some(header),
            Ok(Known::SignedIn { .. } | Known::NeedsSignIn) | Err(_) => return Checking::Nothing,
        };
        Checking::Ask(ToAsk { id, url, header })
    }
}
