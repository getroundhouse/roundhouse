use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaveCancel};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::ToolServerSaveId;

use super::super::sent_call::SentCall;
use super::super::{Client, Reply, Result};
use crate::tool_server_checks::{Checked, Saved, ToSave, When, came_to, saved};

/// A save of a tool server, under way on a thread of its own, as [`ToolServerSaving::start`] starts it. Dropping it
/// before the hub has answered lets go of it, so nothing is added or saved, unless the hub has begun to save it, or a
/// hub from before saves has been sent the add or the edit. Then it's saved all the same, and its reply is its record.
pub struct ToolServerSaving {
    saving: Arc<Saving>,
    client: Client,
}

impl ToolServerSaving {
    /// Starts adding `of`, or saving it, as `when` says, and returns the save, with its reply, which says what came of
    /// it for the server called `name` at `url`, as the person typed them. Once the hub has asked the server for its
    /// tools, that can take 15 seconds. With a hub from before saves, the server is checked, then added or edited, as
    /// that hub has a client do.
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

/// One save, as its thread and whatever lets go of it share it. Each call it sends is sent in order on the client's
/// connection, under a lock that letting go takes too, so the hub has the save before its cancel, and nothing is sent
/// once it has ended.
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

    /// Lets go of it, so nothing is added or saved, unless the hub has begun to save it, or an older hub has been sent
    /// the add or the edit. Then it's saved all the same, and the save answers with its record. It blocks until the hub
    /// has had the cancel.
    pub(super) fn let_go(&self, client: &Client) -> Result<()> {
        let sent = {
            let mut stands = self.stands.lock();
            match std::mem::replace(&mut *stands, Stands::Ended) {
                Stands::Sent => client.send_call(ToolServerSaveCancel { id: self.id.clone() })?,
                Stands::Unsent | Stands::Ended => return Ok(()),
            }
        };
        match sent.reply() {
            // A hub from before saves, whose add or edit isn't sent once this is let go of.
            Err(e) if e.is_unsupported() => Ok(()),
            answer => answer,
        }
    }

    fn saving(&self, client: &Client, of: ToSave, when: When, name: &str, url: &str) -> Result<Saved> {
        let when_sent = match when {
            When::Listed => SaveWhen::Listed,
            When::Anyway => SaveWhen::Anyway,
        };
        let save = ToolServerSave { id: self.id.clone(), of: of.clone().into(), when: when_sent };
        let Some(sent) = self.send(client, save)? else { return Ok(Saved::LetGo) };
        match sent.reply() {
            Err(e) if e.is_unsupported() => self.check_then_save(client, of, when, name, url),
            answer => Ok(saved(answer?, name, url)),
        }
    }

    /// Has the server checked, unless it's saved anyway, then adds or edits it, as a hub from before saves has a client
    /// do. An edit saved anyway says so, so running agents list its tools again.
    fn check_then_save(&self, client: &Client, of: ToSave, when: When, name: &str, url: &str) -> Result<Saved> {
        if when == When::Listed
            && let Checked::Not(why) = came_to(client.tool_server_check(of.clone().into()), name, url)?
        {
            return Ok(Saved::NotListed(why));
        }
        let server = match of {
            ToSave::Add(add) => self.send(client, add)?.map(SentCall::reply),
            ToSave::Edit(edit) => {
                let edit = ToolServerEdit { saved_anyway: when == When::Anyway, ..edit };
                self.send(client, edit)?.map(SentCall::reply)
            }
        };
        Ok(server.transpose()?.map_or(Saved::LetGo, Saved::Saved))
    }

    /// Sends `request`, unless it has ended.
    fn send<R: protocol::client::Request>(&self, client: &Client, request: R) -> Result<Option<SentCall<R>>> {
        let mut stands = self.stands.lock();
        if *stands == Stands::Ended {
            return Ok(None);
        }
        let sent = client.send_call(request)?;
        *stands = Stands::Sent;
        Ok(Some(sent))
    }
}
