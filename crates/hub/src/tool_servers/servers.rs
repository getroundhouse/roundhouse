use std::path::Path;
use std::sync::Arc;

use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Rev, ToolServer, ToolServerId, ToolServerName, ToolServerUrl};

use super::secret::{ToolServerSecret, read};
use crate::login_files::LoginFiles;
use crate::store::{AddingToolServer, Store, StoreError};

/// Adds, removes and tidies one at a time, under its files' lock, so none finds another half done, and no two
/// servers are given names agents would call the same. Synchronous, and only called off the async threads.
pub(crate) struct ToolServers {
    store: Arc<Store>,
    files: LoginFiles<ToolServerSecret>,
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

impl ToolServers {
    /// The tool servers of a hub whose data directory is `data_dir`.
    pub(crate) fn new(store: Arc<Store>, data_dir: &Path) -> Self {
        Self { store, files: LoginFiles::new(data_dir) }
    }

    /// Adds a server, as `ToolServerAdd` says. One of its ID is that server again, whatever else is asked, so it's
    /// looked for before anything is checked. The key is kept, and then the row written, as
    /// [`Locked::connect`](crate::login_files::Locked::connect) says, so a hub that stops between leaves a file no row
    /// names, which tidying removes as the next starts, and no server.
    pub(crate) fn add(&self, asked: ToolServerAdd) -> Result<ToolServerChange, ApiError> {
        let files = self.files.lock();
        let ToolServerAdd { id, name, url, auth } = asked;
        if let Some((server, _)) = self.store.tool_server(&id)? {
            return Ok(ToolServerChange { server, changed: false });
        }
        let name = ToolServerName::typed(&name)?;
        let url = ToolServerUrl::typed(&url)?;
        let (auth, secret) = read(auth)?;
        self.free(&name)?;
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

    /// Forgets a server and its key, as `ToolServerRemove` says. The file goes first, and then the row, as
    /// [`Locked::disconnect`](crate::login_files::Locked::disconnect) says, so a hub that stops between leaves a server
    /// whose key is gone, which tidying removes as the next starts, and never a key that was to be forgotten.
    pub(crate) fn remove(&self, id: &ToolServerId) -> Result<ToolServerRemoved, ApiError> {
        let files = self.files.lock();
        let (server, secret) = self.store.tool_server(id)?.ok_or_else(|| gone(id))?;
        let remove = || Ok(self.store.remove_tool_server(id)?.ok_or_else(|| gone(id))?);
        let rev = match secret {
            Some(file) => files.disconnect(&file, remove)?,
            None => remove()?,
        };
        Ok(ToolServerRemoved { server, rev })
    }

    /// Makes the secrets' files and the servers' rows agree, as the hub starts, as
    /// [`Locked::tidy`](crate::login_files::Locked::tidy) says. A server whose secret is gone is removed, as removing it
    /// would have, had the hub not stopped between letting go of its file and its row.
    pub(crate) fn tidy(&self) -> Result<(), ApiError> {
        let files = self.files.lock();
        files.tidy(self.store.tool_server_secrets()?, |id| {
            self.store.remove_tool_server(id)?;
            Ok(())
        })
    }

    /// Refuses `name` with `Conflict` when agents would call a server the hub has the same, saying which, and that they
    /// would when the names as typed don't show it.
    fn free(&self, name: &ToolServerName) -> Result<(), ApiError> {
        let servers = self.store.tool_servers()?;
        let Some(has_it) = servers.iter().find(|server| server.name.is_same_for_agents(name)) else {
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

fn gone(id: &ToolServerId) -> StoreError {
    StoreError::NotFound { kind: "MCP server", id: id.to_string() }
}
