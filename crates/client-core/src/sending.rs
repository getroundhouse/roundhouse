//! Prompts a person writes, on their way to the hub. Each adds its files to the session, then waits its turn in its
//! chat's line before the hub is sent it, so a chat's prompts are recorded in the order they were written, though an
//! earlier one is still adding a large file. One that doesn't arrive is handed back whole, so nothing written is lost.

use protocol::model::{AttachmentId, ClientMessageId, Seq, Session, SessionId};
use protocol::prompt::check_prompt;

use crate::attachments::FileToSend;
use crate::client::{Client, ClientError, Place, Reply, Result};

/// A prompt as its person wrote it, with what it carries.
pub struct Written {
    pub client_msg_id: ClientMessageId,
    pub text: String,
    pub files: Vec<FileToSend>,
}

/// A prompt the hub didn't take, with why, handed back so it can be offered again.
#[derive(Debug)]
pub struct NotTaken {
    pub error: ClientError,
    pub text: String,
    pub files: Vec<FileToSend>,
}

/// The reply to a prompt started with [`Client::start_sending`] or [`Client::start_queueing`].
pub struct Taking<T> {
    reply: Reply<std::result::Result<T, NotTaken>>,
    /// What it said, for when the call never ran and took its files with it.
    text: String,
}

impl<T> Taking<T> {
    /// What the hub answered, or the prompt handed back.
    pub async fn recv(self) -> std::result::Result<T, NotTaken> {
        match self.reply.recv().await {
            Ok(taken) => taken,
            // Only a call that never ran ends here, and its files went with it.
            Err(error) => Err(NotTaken { error, text: self.text, files: Vec::new() }),
        }
    }

    /// The same, for a test, which has no executor to wait on.
    #[cfg(any(test, feature = "testing"))]
    pub fn wait(self) -> std::result::Result<T, NotTaken> {
        match self.reply.wait() {
            Ok(taken) => taken,
            Err(error) => Err(NotTaken { error, text: self.text, files: Vec::new() }),
        }
    }
}

impl Client {
    /// Sends a prompt as [`Client::send_prompt_carrying`] does, on a thread of its own, as [`Client::start`] starts a
    /// call, in its place in its chat's line from now.
    pub fn start_sending(&self, id: &SessionId, written: Written) -> Taking<(Seq, bool)> {
        self.start_in_line(id, written, |client, place, id, written| {
            let Written { client_msg_id, text, files } = written;
            client.send_in_line(place, id, text, Some(client_msg_id.clone()), files)
        })
    }

    /// Queues a prompt as [`Client::queue_prompt_carrying`] does, on a thread of its own, in its place in its chat's
    /// line from now.
    pub fn start_queueing(&self, id: &SessionId, written: Written) -> Taking<Session> {
        self.start_in_line(id, written, |client, place, id, written| {
            let Written { client_msg_id, text, files } = written;
            client.queue_in_line(place, id, client_msg_id.clone(), text, files)
        })
    }

    /// Adds each file to the session, then sends the prompt that carries them once it's this prompt's turn in its
    /// chat's line. A hub refuses a prompt naming a file it hasn't got, so they go first, in order. With any, `text`
    /// can be empty. The files are only borrowed, so a caller can offer them again if this fails, and their IDs mean
    /// none is added twice.
    pub fn send_prompt_carrying(
        &self,
        id: &SessionId,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
        files: &[FileToSend],
    ) -> Result<(Seq, bool)> {
        self.send_in_line(&self.line_up(id), id, text, client_msg_id, files)
    }

    /// Adds each file to the session, then queues the prompt that carries them, as [`Client::send_prompt_carrying`]
    /// sends one.
    pub fn queue_prompt_carrying(
        &self,
        id: &SessionId,
        client_msg_id: ClientMessageId,
        text: &str,
        files: &[FileToSend],
    ) -> Result<Session> {
        self.queue_in_line(&self.line_up(id), id, client_msg_id, text, files)
    }

    fn start_in_line<T: Send + 'static>(
        &self,
        id: &SessionId,
        written: Written,
        call: fn(&Client, &Place, &SessionId, &Written) -> Result<T>,
    ) -> Taking<T> {
        let place = self.line_up(id);
        let (id, text) = (id.clone(), written.text.clone());
        let reply = self.start(move |client| {
            let taken = call(client, &place, &id, &written);
            Ok(taken.map_err(|error| NotTaken { error, text: written.text, files: written.files }))
        });
        Taking { reply, text }
    }

    fn send_in_line(
        &self,
        place: &Place,
        id: &SessionId,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
        files: &[FileToSend],
    ) -> Result<(Seq, bool)> {
        let carried = self.check_and_add(id, text, files)?;
        place.wait_turn();
        self.session_prompt_with(id, text, client_msg_id, carried)
    }

    fn queue_in_line(
        &self,
        place: &Place,
        id: &SessionId,
        client_msg_id: ClientMessageId,
        text: &str,
        files: &[FileToSend],
    ) -> Result<Session> {
        let carried = self.check_and_add(id, text, files)?;
        place.wait_turn();
        self.session_queue_with(id, client_msg_id, text, carried)
    }

    /// Checks the prompt as the hub would, then adds each of its files to the session, in order, and returns their IDs
    /// for it to name.
    fn check_and_add(&self, id: &SessionId, text: &str, files: &[FileToSend]) -> Result<Vec<AttachmentId>> {
        check_prompt(text, files.len()).map_err(protocol::client::ApiError::from)?;
        for FileToSend { attachment, data } in files {
            self.attachment_add(id, &attachment.id, &attachment.name, attachment.media_type, data.clone())?;
        }
        Ok(files.iter().map(|file| file.attachment.id.clone()).collect())
    }
}

#[cfg(test)]
mod tests;
