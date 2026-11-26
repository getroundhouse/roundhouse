use std::path::Path;
use std::sync::Arc;

use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Rev, ToolServer, ToolServerAuth, ToolServerId, ToolServerName, ToolServerUrl};
use tokio::sync::watch;

use super::SentToolServers;
use super::secret::{ToolServerSecret, read};
use crate::login_files::{Locked, LoginFiles};
use crate::store::{AddingToolServer, Store, StoreError};

/// Adds, edits, removes and tidies one at a time, under its files' lock, so none finds another half done, and no two
/// servers are given names agents would call the same, and has workers sent what each leaves. Synchronous, and only
/// called off the async threads.
pub(crate) struct ToolServers {
    pub(super) store: Arc<Store>,
    pub(super) files: LoginFiles<ToolServerSecret>,
    /// What workers are sent. The keys are in memory only while the hub runs, and in no log.
    pub(super) sent: watch::Sender<SentToolServers>,
}

/// A tool server's record after a change, and whether that changed it, so the hub announces only what did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToolServerChange {
    pub(crate) server: ToolServer,
    pub(crate) changed: bool,
}

/// A tool server's record as it last was, and the revision of its removal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToolServerRemoved {
    pub(crate) server: ToolServer,
    pub(crate) rev: Rev,
}

/// What adding a server would add.
pub(super) enum ToAdd {
    /// The server there is of its ID.
    There(ToolServer),
    New(Adding),
}

/// A server to add, with what it's to know the person by.
pub(super) struct Adding {
    pub(super) id: ToolServerId,
    pub(super) name: ToolServerName,
    pub(super) url: ToolServerUrl,
    pub(super) auth: ToolServerAuth,
    pub(super) secret: Option<ToolServerSecret>,
}

impl ToolServers {
    /// The tool servers of a hub whose data directory is `data_dir`, with what workers are to be sent, before any link
    /// can follow it.
    pub(crate) fn new(store: Arc<Store>, data_dir: &Path) -> Self {
        let servers =
            Self { store, files: LoginFiles::new(data_dir), sent: watch::Sender::new(SentToolServers::default()) };
        servers.publish(&servers.files.lock(), None);
        servers
    }

    /// Adds a server, as `ToolServerAdd` says. One of its ID is that server again, whatever else is asked, so it's
    /// looked for before anything is checked. The key is kept, and then the row written, as
    /// [`Locked::connect`](crate::login_files::Locked::connect) says, so a hub that stops between leaves a file no row
    /// names, which tidying removes as the next starts, and no server.
    pub(crate) fn add(&self, asked: ToolServerAdd) -> Result<ToolServerChange, ApiError> {
        self.changing(|files| self.add_to(files, asked))
    }

    fn add_to(&self, files: &Locked<'_, ToolServerSecret>, asked: ToolServerAdd) -> Result<ToolServerChange, ApiError> {
        let Adding { id, name, url, auth, secret } = match self.adding(asked)? {
            ToAdd::There(server) => return Ok(ToolServerChange { server, changed: false }),
            ToAdd::New(adding) => adding,
        };
        let adding = AddingToolServer { id: &id, name: &name, url: &url, auth: &auth, secret: None };
        let server = match secret {
            Some(secret) => files.connect(&secret, |file| {
                let adding = AddingToolServer { secret: Some(file), ..adding };
                Ok((self.store.add_tool_server(&adding)?, None))
            })?,
            None => self.store.add_tool_server(&adding)?,
        };
        Ok(ToolServerChange { server, changed: true })
    }

    /// What adding `asked` would add, refused as `ToolServerAdd` says.
    pub(super) fn adding(&self, asked: ToolServerAdd) -> Result<ToAdd, ApiError> {
        let ToolServerAdd { id, name, url, auth } = asked;
        if let Some((server, _)) = self.store.tool_server(&id)? {
            return Ok(ToAdd::There(server));
        }
        let name = ToolServerName::typed(&name)?;
        let url = ToolServerUrl::typed(&url)?;
        let (auth, secret) = read(auth)?;
        self.free(&name, &id)?;
        Ok(ToAdd::New(Adding { id, name, url, auth, secret }))
    }

    /// Forgets a server and its key or its sign-in, as `ToolServerRemove` says. The file goes first, and then the row, as
    /// [`Locked::disconnect`](crate::login_files::Locked::disconnect) says, so a hub that stops between leaves a server
    /// whose secret is gone, which tidying removes as the next starts, and never a secret that was to be forgotten.
    pub(crate) fn remove(&self, id: &ToolServerId) -> Result<ToolServerRemoved, ApiError> {
        self.changing(|files| {
            let (server, secret) = self.store.tool_server(id)?.ok_or_else(|| gone(id))?;
            let remove = || Ok(self.store.remove_tool_server(id)?.ok_or_else(|| gone(id))?);
            let rev = match secret {
                Some(file) => files.disconnect(&file, remove)?,
                None => remove()?,
            };
            Ok(ToolServerRemoved { server, rev })
        })
    }

    /// Makes the secrets' files and the servers' rows agree, as the hub starts, as
    /// [`Locked::tidy`](crate::login_files::Locked::tidy) says. A server whose secret is gone is removed, as removing it
    /// would have, had the hub not stopped between letting go of its file and its row. Then each sign-in's record says
    /// what its file does, which a hub that stopped between writing the two left otherwise.
    pub(crate) fn tidy(&self) -> Result<(), ApiError> {
        self.changing(|files| {
            let named = self.store.tool_servers_with_secrets()?.into_iter().map(|(server, file)| (server.id, file));
            files.tidy(named.collect(), |id| {
                self.store.remove_tool_server(id)?;
                Ok(())
            })?;
            self.say_as_kept(files)
        })
    }

    /// Refuses `name` with `Conflict` when agents would call a server other than `id` the same, saying which, and that
    /// they would when the names as typed don't show it.
    pub(super) fn free(&self, name: &ToolServerName, id: &ToolServerId) -> Result<(), ApiError> {
        let servers = self.store.tool_servers()?;
        let Some(has_it) = servers.iter().find(|server| server.id != *id && server.name.is_same_for_agents(name))
        else {
            return Ok(());
        };
        let message = if has_it.name == *name {
            format!("another MCP server is called {name}")
        } else {
            let agents = has_it.name.for_agents();
            format!("agents would call {name} and the MCP server {} the same, {}", has_it.name, agents.as_str())
        };
        Err(ApiError::new(ErrorCode::Conflict, message))
    }
}

pub(super) fn gone(id: &ToolServerId) -> StoreError {
    StoreError::NotFound { kind: "MCP server", id: id.to_string() }
}
