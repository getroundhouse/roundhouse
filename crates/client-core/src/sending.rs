//! Prompts a person writes, on their way to the hub. Each adds its files to the session, then waits its turn in its
//! chat's line before the hub is sent it, so a chat's prompts are recorded in the order they were written, though an
//! earlier one is still adding a large file. One that doesn't arrive is handed back whole, so nothing written is lost.

use protocol::model::{AttachmentId, ClientMessageId, QueuedPrompt, Seq, Session, SessionId};
use protocol::prompt::Prompt;

use crate::attachments::FileToSend;
use crate::client::{Client, ClientError, Place, Reply, Result, Shown, new_message_id};

/// A prompt as its person wrote it, with the bytes of what it carries, under the ID it keeps until it's recorded and
/// after.
pub struct Written {
    pub client_msg_id: ClientMessageId,
    pub prompt: Prompt<FileToSend>,
}

impl Written {
    /// `prompt`, under an ID of its own.
    pub fn new(prompt: Prompt<FileToSend>) -> Self {
        Self { client_msg_id: new_message_id(), prompt }
    }

    /// The prompt as the hub will have it, with its record of each file, to show while it's on its way.
    pub fn on_its_way(&self) -> QueuedPrompt {
        let files = self.prompt.files.iter().map(|file| file.attachment.clone()).collect();
        QueuedPrompt { client_msg_id: self.client_msg_id.clone(), prompt: Prompt::carrying(&*self.prompt.text, files) }
    }
}

/// A prompt the hub didn't take, with why, handed back so it can be offered again.
#[derive(Debug)]
pub struct NotTaken {
    pub error: ClientError,
    pub prompt: Prompt<FileToSend>,
}

/// The reply to a prompt started with [`Client::start_sending`] or [`Client::start_queueing`].
pub struct Taking<T> {
    reply: Reply<std::result::Result<T, NotTaken>>,
    /// What it said, for when the call never ran and took its files with it.
    text: String,
    /// A prompt being queued, which [`Client::queueing`] shows until this is received or let go of.
    _shown: Option<Shown>,
}

impl<T> Taking<T> {
    /// What the hub answered, or the prompt handed back.
    pub async fn recv(self) -> std::result::Result<T, NotTaken> {
        match self.reply.recv().await {
            Ok(taken) => taken,
            // Only a call that never ran ends here, and its files went with it.
            Err(error) => Err(NotTaken { error, prompt: Prompt::new(self.text) }),
        }
    }

    /// The same, for a test, which has no executor to wait on.
    #[cfg(any(test, feature = "testing"))]
    pub fn wait(self) -> std::result::Result<T, NotTaken> {
        match self.reply.wait() {
            Ok(taken) => taken,
            Err(error) => Err(NotTaken { error, prompt: Prompt::new(self.text) }),
        }
    }
}

impl Client {
    /// Sends a prompt as [`Client::send_prompt`] does, on a thread of its own, as [`Client::start`] starts a call, in
    /// its place in its chat's line from now.
    pub fn start_sending(&self, id: &SessionId, written: Written) -> Taking<(Seq, bool)> {
        self.start_in_line(id, written, Client::send_in_line)
    }

    /// Queues a prompt as [`Client::start_sending`] sends one, and [`Client::queueing`] shows it from now until its
    /// reply is received, or the [`Taking`] let go of.
    pub fn start_queueing(&self, id: &SessionId, written: Written) -> Taking<Session> {
        let shown = self.show_queueing(id, written.on_its_way());
        Taking { _shown: Some(shown), ..self.start_in_line(id, written, Client::queue_in_line) }
    }

    /// Adds each file the hub hasn't been given to the session, then sends the prompt that carries them once it's this
    /// prompt's turn in its chat's line, and waits for it to be recorded. A hub refuses a prompt naming a file it
    /// hasn't got, so they go first, in order. The prompt is only borrowed, so a caller can offer it again if this
    /// fails, and its ID, and its files' IDs, mean nothing is recorded or added twice.
    pub fn send_prompt(&self, id: &SessionId, written: &Written) -> Result<(Seq, bool)> {
        self.send_in_line(&self.line_up(id), id, written)
    }

    fn start_in_line<T: Send + 'static>(
        &self,
        id: &SessionId,
        written: Written,
        call: fn(&Client, &Place, &SessionId, &Written) -> Result<T>,
    ) -> Taking<T> {
        let place = self.line_up(id);
        let (id, text) = (id.clone(), written.prompt.text.clone());
        let reply = self.start(move |client| {
            let taken = call(client, &place, &id, &written);
            Ok(taken.map_err(|error| NotTaken { error, prompt: written.prompt }))
        });
        Taking { reply, text, _shown: None }
    }

    fn send_in_line(&self, place: &Place, id: &SessionId, written: &Written) -> Result<(Seq, bool)> {
        let client_msg_id = Some(written.client_msg_id.clone());
        self.carrying(place, id, &written.prompt, |carried| self.prompt_carried(id, client_msg_id.clone(), carried))
    }

    fn queue_in_line(&self, place: &Place, id: &SessionId, written: &Written) -> Result<Session> {
        let client_msg_id = &written.client_msg_id;
        self.carrying(place, id, &written.prompt, |carried| self.queue_carried(id, client_msg_id.clone(), carried))
    }

    /// Checks the prompt as the hub would, adds each of its files the hub hasn't been given, in order, then once it's
    /// this prompt's turn makes `call` with the prompt as it names them. A file this connection already gave the hub,
    /// such as one kept with a draft, isn't sent again. If the hub says one isn't there after all, such as one it has
    /// let go of since, each is added and the call made once more: the hub refuses a prompt naming a file it hasn't got
    /// before it records anything, so trying again can't record it twice.
    fn carrying<T>(
        &self,
        place: &Place,
        id: &SessionId,
        prompt: &Prompt<FileToSend>,
        call: impl Fn(Prompt<AttachmentId>) -> Result<T>,
    ) -> Result<T> {
        prompt.check().map_err(protocol::client::ApiError::from)?;
        let files = &prompt.files;
        let given: Vec<&FileToSend> = files.iter().filter(|file| self.has_given(id, &file.attachment.id)).collect();
        self.add_files(id, files.iter().filter(|file| !self.has_given(id, &file.attachment.id)))?;
        let carried = Prompt::carrying(&*prompt.text, files.iter().map(|file| file.attachment.id.clone()).collect());
        place.wait_turn();
        match call(carried.clone()) {
            Err(e) if e.is_not_found() && !given.is_empty() => {
                for file in &given {
                    self.forget_given(id, &file.attachment.id);
                }
                self.add_files(id, given.into_iter())?;
                call(carried)
            }
            answered => answered,
        }
    }

    /// Adds each file to the session, in order.
    fn add_files<'a>(&self, id: &SessionId, files: impl Iterator<Item = &'a FileToSend>) -> Result<()> {
        for file in files {
            let FileToSend { attachment, data } = file;
            file.give(|kind| self.attachment_add(id, &attachment.id, &attachment.name, kind, data.clone()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
