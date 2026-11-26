//! Which tool servers workers are sent, worked out in one place, under the servers' lock, and kept as it changes for each
//! worker's link to follow.

use std::collections::HashMap;
use std::fmt;

use protocol::model::{ToolServer, ToolServerAuth, ToolServerId, ToolSignInId};
use protocol::worker::{ProxiedServer, ServerHeader, ServerHeaderValue, ServerSignIn, TokenNumber, ToolServersSet};
use tokio::sync::watch;
use tracing::{error, warn};

use super::as_sent::Change;
use super::secret::{ToolServerSecret, as_sent};
use super::{ServersAsSent, ToolServers};
use crate::login_files::{Locked, LoginFile, Stored};
use crate::store::StoreError;

/// The tool servers workers are sent, with their keys, and those they aren't, and why. Not `Debug`, since nothing but a
/// worker is told of the first.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SentToolServers {
    sent: Vec<ProxiedServer>,
    as_sent: ServersAsSent,
    /// The last change `as_sent` numbers.
    last: Change,
    left_out: Vec<(ToolServerId, LeftOut)>,
}

impl SentToolServers {
    /// What a worker is set.
    pub(crate) fn set(&self) -> ToolServersSet {
        ToolServersSet { servers: self.sent.clone() }
    }

    /// How each is, for a worker that's set them to have them so.
    pub(crate) fn as_sent(&self) -> &ServersAsSent {
        &self.as_sent
    }

    /// Adds `server`, known by the sign-in `signed_in_by` if it's signed in to, to what workers are sent after `before`,
    /// as [`ServersAsSent::after`] says.
    fn add(
        &mut self,
        before: &SentToolServers,
        server: ProxiedServer,
        signed_in_by: Option<ToolSignInId>,
        saved_anyway: Option<&ToolServerId>,
    ) {
        let was = before.sent.iter().find(|was| was.id == server.id);
        let saved_anyway = saved_anyway == Some(&server.id);
        let as_sent = before.as_sent.after(was, &server, signed_in_by, saved_anyway, &mut self.last);
        self.as_sent.set(server.id.clone(), as_sent);
        self.sent.push(server);
    }
}

/// Why workers aren't sent a server, which the log says, so never with what was read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LeftOut {
    /// Only a newer build knows how it knows the person.
    UnknownAuth,
    /// Only a newer build can read its key or its sign-in.
    Newer,
    /// Its key or its sign-in is gone.
    Gone,
    /// Its file can't be read.
    Unreadable(std::io::ErrorKind),
}

impl fmt::Display for LeftOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeftOut::UnknownAuth => f.write_str("only a newer build knows how it knows the person"),
            LeftOut::Newer => f.write_str("only a newer build can read its key or its sign-in"),
            LeftOut::Gone => f.write_str("its key or its sign-in is gone"),
            LeftOut::Unreadable(kind) => write!(f, "its file can't be read: {kind}"),
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

    /// The server `id` as workers are sent it, or `None` when they aren't, or the hub hasn't it.
    pub(crate) fn sent_server(&self, id: &ToolServerId) -> Option<ProxiedServer> {
        self.sent.borrow().sent.iter().find(|server| server.id == *id).cloned()
    }

    /// Runs `change` under the servers' lock, then, still under it, works out what workers are to be sent, whether
    /// `change` worked or not, since one that failed part way can have changed something.
    pub(super) fn changing<T, E>(
        &self,
        change: impl FnOnce(&Locked<'_, ToolServerSecret>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.changing_saved(None, change)
    }

    /// The same, where the person saves what changes of the server `saved_anyway` though the hub couldn't use it.
    pub(super) fn changing_saved<T, E>(
        &self,
        saved_anyway: Option<&ToolServerId>,
        change: impl FnOnce(&Locked<'_, ToolServerSecret>) -> Result<T, E>,
    ) -> Result<T, E> {
        let files = self.files.lock();
        let changed = change(&files);
        self.publish(&files, saved_anyway);
        changed
    }

    /// Has workers sent the servers the hub has now, if that has changed, and says each that's left out. If they can't
    /// be read, workers have what they were last sent until the next change.
    pub(super) fn publish(&self, files: &Locked<'_, ToolServerSecret>, saved_anyway: Option<&ToolServerId>) {
        let worked_out = self.worked_out(files, &self.sent.borrow(), saved_anyway);
        let worked_out = match worked_out {
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

    /// What workers are to be sent after `before`.
    fn worked_out(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        before: &SentToolServers,
        saved_anyway: Option<&ToolServerId>,
    ) -> Result<SentToolServers, StoreError> {
        let named = self.store.tool_servers_with_secrets()?.into_iter().map(|(server, file)| (server.id, file));
        let secrets: HashMap<_, _> = named.collect();
        let mut worked_out = SentToolServers { last: before.last, ..SentToolServers::default() };
        for server in self.store.tool_servers()? {
            let file = secrets.get(&server.id).and_then(Option::as_ref);
            match knows_by(&server.auth, || secret(files, file)) {
                Ok(known) => {
                    let signed_in_by = if let Known::SignedIn { by, .. } = &known { Some(by.clone()) } else { None };
                    worked_out.add(before, proxied(server, known), signed_in_by, saved_anyway);
                }
                Err(why) => worked_out.left_out.push((server.id, why)),
            }
        }
        Ok(worked_out)
    }
}

/// What a server knows the person by, as workers are sent it.
pub(super) enum Known {
    Not,
    By(ServerHeader),
    /// Its access token, which is the token `number` of those it has had, given by a sign-in and what renewed it since.
    SignedIn {
        header: ServerHeader,
        number: TokenNumber,
        by: ToolSignInId,
    },
    NeedsSignIn,
}

/// What a server that knows the person by `auth` knows them by, with the key or the access token `secret` gives.
pub(super) fn knows_by(
    auth: &ToolServerAuth,
    secret: impl FnOnce() -> Result<ToolServerSecret, LeftOut>,
) -> Result<Known, LeftOut> {
    match auth {
        ToolServerAuth::Key { header } => {
            let ToolServerSecret::Key(key) = secret()? else { return Err(LeftOut::Gone) };
            let value = ServerHeaderValue::new(as_sent(header, &key));
            Ok(Known::By(ServerHeader { name: header.clone(), value }))
        }
        // Its file says how it stands, and its record says what the file does.
        ToolServerAuth::SignIn { status: _ } => {
            let ToolServerSecret::SignIn(kept) = secret()? else { return Err(LeftOut::Gone) };
            Ok(match kept.tokens {
                Some(tokens) => Known::SignedIn {
                    header: ServerHeader::bearer(tokens.access.as_str()),
                    number: kept.number,
                    by: kept.made_by,
                },
                None => Known::NeedsSignIn,
            })
        }
        ToolServerAuth::Neither => Ok(Known::Not),
        ToolServerAuth::Unknown => Err(LeftOut::UnknownAuth),
    }
}

pub(super) fn secret(
    files: &Locked<'_, ToolServerSecret>,
    file: Option<&LoginFile>,
) -> Result<ToolServerSecret, LeftOut> {
    match files.read(file.ok_or(LeftOut::Gone)?) {
        Ok(Stored::Secret(secret)) => Ok(secret),
        Ok(Stored::Newer) => Err(LeftOut::Newer),
        Ok(Stored::Nothing) => Err(LeftOut::Gone),
        Err(e) => Err(LeftOut::Unreadable(e.kind())),
    }
}

fn proxied(server: ToolServer, known: Known) -> ProxiedServer {
    let (header, sign_in) = match known {
        Known::Not => (None, None),
        Known::By(header) => (Some(header), None),
        Known::SignedIn { header, number, by: _ } => (Some(header), Some(ServerSignIn::Works { token: number })),
        Known::NeedsSignIn => (None, Some(ServerSignIn::NeedsSignIn)),
    };
    let ToolServer { id, name, url, auth: _, rev: _ } = server;
    ProxiedServer { id, name: name.for_agents(), label: name, url, header, sign_in }
}
