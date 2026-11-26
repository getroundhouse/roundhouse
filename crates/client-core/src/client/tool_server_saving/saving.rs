use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaveCancel};
use protocol::model::ToolServerSaveId;

use super::super::{Client, Reply, Result};
use crate::tool_server_checks::{Saved, ToSave, When, saved};

/// A save of a tool server, under way on a thread of its own, as [`ToolServerSaving::start`] starts it. Dropping it
/// before the hub has answered lets go of it, so nothing is added or saved, unless the hub has begun to save it. Then
/// it's saved all the same, and its reply is its record.
pub struct ToolServerSaving {
    saving: Arc<Saving>,
    client: Client,
}

impl ToolServerSaving {
    /// Starts adding `of`, or saving it, as `when` says, and returns the save, with its reply, which says what came of
    /// it for the server called `name` at `url`, as the person typed them. Once the hub has asked the server for its
    /// tools, that can take 15 seconds. A hub from before saves refuses it with `Unsupported`.
    pub fn start(client: &Client, of: ToSave, when: When, name: String, url: String) -> (Self, Reply<Saved>) {
        let saving = Arc::new(Saving::default());
        let on = Arc::clone(&saving);
        let reply = client.start(move |client| on.save(client, of, when, &name, &url));
        (Self { saving, client: client.clone() }, reply)
    }
}

impl Drop for ToolServerSaving {
    /// Lets go of it on a thread of its own, since that waits for the hub to have had the cancel, and quitting waits
    /// for it, as for any call that changes something. One that has been answered has nothing to let go of.
    fn drop(&mut self) {
        if self.saving.is_under_way() {
            let saving = Arc::clone(&self.saving);
            drop(self.client.start(move |client| saving.let_go(client)));
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
    pub(super) fn save(&self, client: &Client, of: ToSave, when: When, name: &str, url: &str) -> Result<Saved> {
        let saved = self.saving(client, of, when, name, url);
        *self.stands.lock() = Stands::Ended;
        saved
    }

    /// Lets go of it, so nothing is added or saved, unless the hub has begun to save it. Then it's saved all the same,
    /// and the save answers with its record. It blocks until the hub has had the cancel.
    pub(super) fn let_go(&self, client: &Client) -> Result<()> {
        let sent = {
            let mut stands = self.stands.lock();
            match std::mem::replace(&mut *stands, Stands::Ended) {
                Stands::Sent => client.send_call(ToolServerSaveCancel { id: self.id.clone() })?,
                Stands::Unsent | Stands::Ended => return Ok(()),
            }
        };
        sent.reply()
    }

    fn saving(&self, client: &Client, of: ToSave, when: When, name: &str, url: &str) -> Result<Saved> {
        let when = match when {
            When::Listed => SaveWhen::Listed,
            When::Anyway => SaveWhen::Anyway,
        };
        let sent = {
            let mut stands = self.stands.lock();
            if *stands == Stands::Ended {
                return Ok(Saved::LetGo);
            }
            let sent = client.send_call(ToolServerSave { id: self.id.clone(), of: of.into(), when })?;
            *stands = Stands::Sent;
            sent
        };
        Ok(saved(sent.reply()?, name, url))
    }
}
