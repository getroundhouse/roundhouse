//! Editing a server: what a person changes of it, what it's to know them by then, and in what order that's kept. What a
//! server knows the person by is never sent to another, so moving one takes what the other is to know them by.

use protocol::client::tool_servers::ToolServerEdit;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    ToolServer, ToolServerAuth, ToolServerAuthEdit, ToolServerHeader, ToolServerId, ToolServerKey, ToolServerName,
    ToolServerUrl,
};

use super::secret::{ToolServerSecret, read_key, refused, unknown_way};
use super::servers::gone;
use super::{ToolServerChange, ToolServers};
use crate::error::internal;
use crate::login_files::{Locked, LoginFile, Stored};
use crate::store::{EditingToolServer, KnownBy};

/// What a person changes of a server's name and URL.
#[derive(Debug)]
pub(super) struct Changes {
    pub(super) name: Option<ToolServerName>,
    pub(super) url: Option<ToolServerUrl>,
}

impl Changes {
    /// The URL `server` has once these are made.
    pub(super) fn url_of<'a>(&'a self, server: &'a ToolServer) -> &'a ToolServerUrl {
        self.url.as_ref().unwrap_or(&server.url)
    }

    /// Whether they make `server` another.
    pub(super) fn move_it(&self, server: &ToolServer) -> bool {
        !self.url_of(server).is_same_server(&server.url)
    }

    fn editing<'a>(&'a self, known_by: Option<KnownBy<'a>>) -> EditingToolServer<'a> {
        EditingToolServer { name: self.name.as_ref(), url: self.url.as_ref(), known_by }
    }
}

/// A server there is, as it's to be edited without signing in.
pub(super) struct Editing {
    pub(super) server: ToolServer,
    pub(super) file: Option<LoginFile>,
    pub(super) changes: Changes,
    pub(super) becomes: Becomes,
}

/// How a server is to know the person, as it's edited without signing in.
pub(super) enum Becomes {
    AsItIs,
    /// By the key it has, sent in `header`.
    KeptKey {
        header: ToolServerHeader,
    },
    Key {
        header: ToolServerHeader,
        key: ToolServerKey,
    },
    Neither,
}

impl ToolServers {
    /// Edits a server, as `ToolServerEdit` says.
    pub(crate) fn edit(&self, asked: ToolServerEdit) -> Result<ToolServerChange, ApiError> {
        self.changing(|files| self.edit_in(files, asked))
    }

    fn edit_in(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        asked: ToolServerEdit,
    ) -> Result<ToolServerChange, ApiError> {
        let Editing { server, file, changes, becomes } = self.editing(asked)?;
        match becomes {
            Becomes::AsItIs => self.keep_edit(server, &changes, None),
            Becomes::KeptKey { header } => {
                let auth = ToolServerAuth::Key { header };
                self.keep_edit(server, &changes, Some(KnownBy { auth: &auth, secret: file.as_ref() }))
            }
            Becomes::Key { header, key } => {
                let secret = ToolServerSecret::Key(key);
                let auth = ToolServerAuth::Key { header };
                self.keep_secret(files, (server, file), &changes, &auth, &secret)
            }
            Becomes::Neither => files.unname(|| {
                let known_by = KnownBy { auth: &ToolServerAuth::Neither, secret: None };
                Ok((self.keep_edit(server, &changes, Some(known_by))?, file))
            }),
        }
    }

    /// What editing `asked` would change, refused as `ToolServerEdit` says.
    pub(super) fn editing(&self, asked: ToolServerEdit) -> Result<Editing, ApiError> {
        let ToolServerEdit { id, name, url, auth } = asked;
        let (server, file) = self.store.tool_server(&id)?.ok_or_else(|| gone(&id))?;
        let changes = self.changes(&id, name, url)?;
        let becomes = becomes(&server, &changes, auth)?;
        Ok(Editing { server, file, changes, becomes })
    }

    /// What's to change of the server `id`, as a person typed its name and its URL, refused as `ToolServerAdd` says of
    /// each.
    pub(super) fn changes(
        &self,
        id: &ToolServerId,
        name: Option<String>,
        url: Option<String>,
    ) -> Result<Changes, ApiError> {
        let name = name.map(|name| ToolServerName::typed(&name)).transpose()?;
        let url = url.map(|url| ToolServerUrl::typed(&url)).transpose()?;
        if let Some(name) = &name {
            self.free(name, id)?;
        }
        Ok(Changes { name, url })
    }

    /// Keeps `changes` to a server, which its row and the file it names are, with `secret`, which it's to know the
    /// person by as `auth` says. A secret takes the place of another in a file of its own, named by the row as it
    /// changes, so a hub that stops between leaves the server as it was and a file no row names, which tidying removes.
    /// So a server that's moved has another file, and whatever goes by its file finds that it's another. Where the
    /// secret is all that changes, it's written in place of the one there was, so the record, which says nothing of it,
    /// stays as it is.
    pub(super) fn keep_secret(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        (server, file): (ToolServer, Option<LoginFile>),
        changes: &Changes,
        auth: &ToolServerAuth,
        secret: &ToolServerSecret,
    ) -> Result<ToolServerChange, ApiError> {
        if let Some(file) = &file
            && !changes.move_it(&server)
        {
            let in_place = Some(KnownBy { auth, secret: Some(file) });
            if matches!(files.read(file), Ok(Stored::Secret(held)) if held == *secret) {
                return self.keep_edit(server, changes, in_place);
            }
            let as_it_was = changes.name.as_ref().is_none_or(|name| *name == server.name)
                && changes.url.as_ref().is_none_or(|url| *url == server.url)
                && *auth == server.auth;
            if as_it_was {
                files.replace(file, secret).map_err(|e| internal(format!("couldn't keep the secret: {e}")))?;
                return Ok(ToolServerChange { server, changed: false });
            }
        }
        files.connect(secret, |new| {
            let kept = self.keep_edit(server, changes, Some(KnownBy { auth, secret: Some(new) }))?;
            Ok((kept, file))
        })
    }

    /// Writes `changes` to the row of `server`, and how it's to know the person, where that changes.
    fn keep_edit(
        &self,
        server: ToolServer,
        changes: &Changes,
        known_by: Option<KnownBy<'_>>,
    ) -> Result<ToolServerChange, ApiError> {
        Ok(match self.store.edit_tool_server(&server.id, &changes.editing(known_by))? {
            Some(server) => ToolServerChange { server, changed: true },
            None => ToolServerChange { server, changed: false },
        })
    }
}

/// How `server` is to know the person once `changes` are made, as a client gave that in `auth`, refused as
/// `ToolServerEdit` says.
fn becomes(server: &ToolServer, changes: &Changes, auth: Option<ToolServerAuthEdit>) -> Result<Becomes, ApiError> {
    let moved = changes.move_it(server);
    match auth {
        None => match &server.auth {
            ToolServerAuth::Key { .. } if moved => Err(refused("that's another server, so give it its key")),
            ToolServerAuth::SignIn { .. } if moved => Err(refused("that's another server, so sign in to it")),
            ToolServerAuth::Unknown if moved => {
                Err(refused("that's another server, so say how it's to know who you are"))
            }
            ToolServerAuth::Key { .. }
            | ToolServerAuth::SignIn { .. }
            | ToolServerAuth::Neither
            | ToolServerAuth::Unknown => Ok(Becomes::AsItIs),
        },
        Some(ToolServerAuthEdit::Key { header, key, url }) => {
            let header = ToolServerHeader::typed(&header)?;
            if !ToolServerUrl::typed(&url)?.is_same_server(changes.url_of(server)) {
                let message = "the MCP server has been moved to another meanwhile, so look at where it is";
                return Err(ApiError::new(ErrorCode::Conflict, message));
            }
            match key {
                Some(key) => Ok(Becomes::Key { header, key: read_key(&key)? }),
                None if moved => Err(refused("that's another server, so give it its key")),
                None => match server.auth {
                    ToolServerAuth::Key { .. } => Ok(Becomes::KeptKey { header }),
                    ToolServerAuth::SignIn { .. } | ToolServerAuth::Neither | ToolServerAuth::Unknown => {
                        Err(refused("the MCP server has no key to keep, so give it one"))
                    }
                },
            }
        }
        Some(ToolServerAuthEdit::Neither) => Ok(Becomes::Neither),
        Some(ToolServerAuthEdit::Unknown) => Err(unknown_way()),
    }
}
