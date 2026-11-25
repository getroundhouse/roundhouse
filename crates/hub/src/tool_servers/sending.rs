//! Which tool servers workers are sent, worked out in one place, under the servers' lock, and kept as it changes for each
//! worker's link to follow.

use std::collections::HashMap;
use std::fmt;

use protocol::model::{ToolServer, ToolServerAuth, ToolServerId, ToolServerKey};
use protocol::worker::{ProxiedServer, ServerHeader, ServerHeaderValue, ToolServersSet};
use tokio::sync::watch;
use tracing::{error, warn};

use super::ToolServers;
use super::secret::ToolServerSecret;
use crate::login_files::{Locked, LoginFile, Stored};
use crate::store::StoreError;

/// The tool servers workers are sent, with their keys, and those they aren't, and why. Not `Debug`, since nothing but a
/// worker is told of the first.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SentToolServers {
    sent: Vec<ProxiedServer>,
    left_out: Vec<(ToolServerId, LeftOut)>,
}

impl SentToolServers {
    /// What a worker is set.
    pub(crate) fn set(&self) -> ToolServersSet {
        ToolServersSet { servers: self.sent.clone() }
    }
}

/// Why workers aren't sent a server, which the log says, so never with what was read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LeftOut {
    /// Only a newer build knows how it knows the person.
    UnknownAuth,
    /// Only a newer build can read its key.
    Newer,
    /// Its key is gone.
    Gone,
    /// Its key's file can't be read.
    Unreadable(std::io::ErrorKind),
}

impl fmt::Display for LeftOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeftOut::UnknownAuth => f.write_str("only a newer build knows how it knows the person"),
            LeftOut::Newer => f.write_str("only a newer build can read its key"),
            LeftOut::Gone => f.write_str("its key is gone"),
            LeftOut::Unreadable(kind) => write!(f, "its key's file can't be read: {kind}"),
        }
    }
}

impl ToolServers {
    /// The tool servers workers are to have, from now on, as they change. They change under the servers' lock, as each
    /// change to the servers is made, so they change in the order those were made. Each worker's link follows them on a
    /// task of its own, so nothing is sent to a worker under the lock, and no change waits on a worker.
    pub(crate) fn sent(&self) -> watch::Receiver<SentToolServers> {
        self.sent.subscribe()
    }

    /// Runs `change` under the servers' lock, then, still under it, works out what workers are to be sent, whether
    /// `change` worked or not, since one that failed part way can have changed something.
    pub(super) fn changing<T, E>(
        &self,
        change: impl FnOnce(&Locked<'_, ToolServerSecret>) -> Result<T, E>,
    ) -> Result<T, E> {
        let files = self.files.lock();
        let changed = change(&files);
        self.publish(&files);
        changed
    }

    /// Has workers sent the servers the hub has now, if that has changed, and says each that's left out. If they can't
    /// be read, workers have what they were last sent until the next change.
    pub(super) fn publish(&self, files: &Locked<'_, ToolServerSecret>) {
        let worked_out = match self.worked_out(files) {
            Ok(worked_out) => worked_out,
            Err(error) => {
                error!(
                    name: "tool_servers.read_failed",
                    %error,
                    "Couldn't read the tool servers, so workers have those they were last sent"
                );
                return;
            }
        };
        self.sent.send_if_modified(|current| {
            if *current == worked_out {
                return false;
            }
            for (id, why) in &worked_out.left_out {
                warn!(name: "tool_servers.left_out", %id, %why, "Workers aren't sent a tool server");
            }
            *current = worked_out;
            true
        });
    }

    fn worked_out(&self, files: &Locked<'_, ToolServerSecret>) -> Result<SentToolServers, StoreError> {
        let secrets: HashMap<_, _> = self.store.tool_server_secrets()?.into_iter().collect();
        let mut worked_out = SentToolServers::default();
        for server in self.store.tool_servers()? {
            let file = secrets.get(&server.id).and_then(Option::as_ref);
            match header(&server.auth, files, file) {
                Ok(header) => worked_out.sent.push(proxied(server, header)),
                Err(why) => worked_out.left_out.push((server.id, why)),
            }
        }
        Ok(worked_out)
    }
}

/// What each request to a server that knows the person by `auth` carries, with the key `file` holds.
fn header(
    auth: &ToolServerAuth,
    files: &Locked<'_, ToolServerSecret>,
    file: Option<&LoginFile>,
) -> Result<Option<ServerHeader>, LeftOut> {
    match auth {
        ToolServerAuth::Key { header } => {
            let key = key(files, file)?;
            Ok(Some(ServerHeader { name: header.clone(), value: ServerHeaderValue::new(key.as_str()) }))
        }
        ToolServerAuth::Neither => Ok(None),
        ToolServerAuth::Unknown => Err(LeftOut::UnknownAuth),
    }
}

fn key(files: &Locked<'_, ToolServerSecret>, file: Option<&LoginFile>) -> Result<ToolServerKey, LeftOut> {
    match files.read(file.ok_or(LeftOut::Gone)?) {
        Ok(Stored::Secret(ToolServerSecret::Key(key))) => Ok(key),
        Ok(Stored::Newer) => Err(LeftOut::Newer),
        Ok(Stored::Nothing) => Err(LeftOut::Gone),
        Err(e) => Err(LeftOut::Unreadable(e.kind())),
    }
}

fn proxied(server: ToolServer, header: Option<ServerHeader>) -> ProxiedServer {
    let ToolServer { id, name, url, auth: _, rev: _ } = server;
    ProxiedServer { id, name: name.for_agents(), label: name, url, header }
}
