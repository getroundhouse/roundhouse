use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaveCancel};
use protocol::model::ToolServerSaveId;

use super::super::{Connection, Reply, Result};
use crate::tool_server_checks::{Saved, saved};

/// A save of a tool server, under way on a thread of its own, as [`ToolServerSaving::start`] starts it. Dropping it
/// before the hub has answered lets go of it, so nothing is added or saved, unless the hub has begun to save it. Then
/// it's saved all the same, and its reply is its record.
pub struct ToolServerSaving {
    saving: Arc<Saving>,
    /// The connection the save went on, which its cancel goes on after it. `None` for one never sent.
    connection: Option<Connection>,
}

impl ToolServerSaving {
    /// Starts adding `of`, or saving it, as `when` says, and returns the save, with its reply, which says what came of
    /// it for the server called `name` at `url`, as the person typed them. Once the hub has asked the server for its
    /// tools, that can take 15 seconds. A hub from before saves refuses it with `Unsupported`.
    /// It's sent on the connection `hub` has that has caught up, and fails at once without one, as
    /// [`Hub::ask`](crate::Hub::ask) says.
    pub fn start(
        hub: &crate::Hub,
        of: ToolServerToCheck,
        when: SaveWhen,
        name: String,
        url: String,
    ) -> (Self, Reply<Saved>) {
        match hub.ready() {
            Ok(connection) => Self::on(&connection, of, when, name, url),
            Err(e) => (Self { saving: Arc::new(Saving::default()), connection: None }, Reply::now(Err(e))),
        }
    }

    /// As [`ToolServerSaving::start`], on `connection`.
    pub(crate) fn on(
        connection: &Connection,
        of: ToolServerToCheck,
        when: SaveWhen,
        name: String,
        url: String,
    ) -> (Self, Reply<Saved>) {
        let saving = Arc::new(Saving::default());
        let on = Arc::clone(&saving);
        let reply = connection.start(move |connection| on.save(connection, of, when, &name, &url));
        (Self { saving, connection: Some(connection.clone()) }, reply)
    }
}

impl Drop for ToolServerSaving {
    /// Lets go of it on a thread of its own, since that waits for the hub to have had the cancel, and quitting waits
    /// for it, as for any call that changes something. One that has been answered has nothing to let go of.
    fn drop(&mut self) {
        if let Some(connection) = self.connection.as_ref().filter(|_| self.saving.is_under_way()) {
            let saving = Arc::clone(&self.saving);
            drop(connection.start(move |connection| saving.let_go(connection)));
        }
    }
}

/// One save, as its thread and whatever lets go of it share it. The save and its cancel are sent in order on the
/// client's connection, under a lock that letting go takes too, so the hub has the save before its cancel, and nothing
/// is sent once it has ended.
pub(super) struct Saving {
    id: ToolServerSaveId,
    stands: Mutex<Stands>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stands {
    /// Nothing has been sent yet.
    Unsent,
    Sent,
    /// It has been let go of, or answered, so nothing more is sent.
    Ended,
}

impl Default for Saving {
    fn default() -> Self {
        let id = ToolServerSaveId::from(uuid::Uuid::new_v4().to_string());
        Self { id, stands: Mutex::new(Stands::Unsent) }
    }
}

impl Saving {
    fn is_under_way(&self) -> bool {
        *self.stands.lock() != Stands::Ended
    }

    /// Adds or saves `of`, and answers with what came of it, blocking until the hub has. Nothing is sent once it has
    /// ended, and it ends as it answers.
    pub(super) fn save(
        &self,
        connection: &Connection,
        of: ToolServerToCheck,
        when: SaveWhen,
        name: &str,
        url: &str,
    ) -> Result<Saved> {
        let saved = self.saving(connection, of, when, name, url);
        *self.stands.lock() = Stands::Ended;
        saved
    }

    /// Lets go of it, so nothing is added or saved, unless the hub has begun to save it. Then it's saved all the same,
    /// and the save answers with its record. It blocks until the hub has had the cancel.
    pub(super) fn let_go(&self, connection: &Connection) -> Result<()> {
        let sent = {
            let mut stands = self.stands.lock();
            match std::mem::replace(&mut *stands, Stands::Ended) {
                Stands::Sent => connection.send_call(ToolServerSaveCancel { id: self.id.clone() })?,
                Stands::Unsent | Stands::Ended => return Ok(()),
            }
        };
        sent.reply()
    }

    fn saving(
        &self,
        connection: &Connection,
        of: ToolServerToCheck,
        when: SaveWhen,
        name: &str,
        url: &str,
    ) -> Result<Saved> {
        let sent = {
            let mut stands = self.stands.lock();
            if *stands == Stands::Ended {
                return Ok(Saved::LetGo);
            }
            let sent = connection.send_call(ToolServerSave { id: self.id.clone(), of, when })?;
            *stands = Stands::Sent;
            sent
        };
        Ok(saved(sent.reply()?, name, url))
    }
}
