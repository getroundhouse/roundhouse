//! Prompts being queued from here that the hub hasn't answered for yet, such as while their files are added, so a
//! client can show each at the end of its chat's queue, where the hub will have it, from the moment it's written.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::{ClientMessageId, QueuedPrompt, Session, SessionId};

use super::Client;

/// Each chat's prompts being queued from here, oldest first.
#[derive(Default)]
pub(crate) struct Queueing(Mutex<HashMap<SessionId, Vec<QueuedPrompt>>>);

/// One prompt being queued, which shows until this goes.
pub(crate) struct Shown {
    queueing: Arc<Queueing>,
    session_id: SessionId,
    client_msg_id: ClientMessageId,
}

impl Client {
    /// Shows `prompt` at the end of the chat's queue until what this returns goes.
    pub(crate) fn show_queueing(&self, session_id: &SessionId, prompt: QueuedPrompt) -> Shown {
        let client_msg_id = prompt.client_msg_id.clone();
        self.inner.queueing.0.lock().entry(session_id.clone()).or_default().push(prompt);
        Shown { queueing: self.inner.queueing.clone(), session_id: session_id.clone(), client_msg_id }
    }

    /// What's being queued from here for `session` that isn't on its record yet, oldest first, for a client to show
    /// at the end of its queue: each prompt [`Client::start_queueing`] was given, until its reply is received. One the
    /// record has shows there, and not again here.
    pub fn queueing(&self, session: &Session) -> Vec<QueuedPrompt> {
        let on_record = |prompt: &QueuedPrompt| session.queued.iter().any(|q| q.client_msg_id == prompt.client_msg_id);
        let queueing = self.inner.queueing.0.lock();
        let prompts = queueing.get(&session.id).into_iter().flatten();
        prompts.filter(|prompt| !on_record(prompt)).cloned().collect()
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        let mut queueing = self.queueing.0.lock();
        let Some(prompts) = queueing.get_mut(&self.session_id) else { return };
        prompts.retain(|prompt| prompt.client_msg_id != self.client_msg_id);
        if prompts.is_empty() {
            queueing.remove(&self.session_id);
        }
    }
}
