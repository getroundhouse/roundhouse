//! What's sent from a chat: its prompts, with what they carry, and Stop.

use client_core::{FileToSend, NotTaken, Written};
use gpui_kit::Context;
use protocol::model::WaitId;
use protocol::prompt::Prompt;

use super::{Transcript, TranscriptEvent};
use crate::attachments::Picture;

/// What Retry sends, which has the agent pick up where its request failed, as a message does for its CLI.
const RETRY: &str = "Continue";

impl Transcript {
    /// Shows the prompt at once, and takes it back if it can't be sent. Returns false if it's empty.
    pub fn send_prompt(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        self.send_prompt_with(Prompt::new(text), cx)
    }

    /// Sends the prompt with what it carries, which the chat shows at once, on its way, as
    /// [`client_core::OpenSession::start_sending`] says. If it isn't taken, it stops showing and is handed back with
    /// [`TranscriptEvent::NotSent`]. Returns false if there's nothing to send.
    pub fn send_prompt_with(&mut self, prompt: Prompt<FileToSend>, cx: &mut Context<Self>) -> bool {
        self.send(prompt, |message, prompt| TranscriptEvent::NotSent { message, prompt }, cx)
    }

    /// Sends the prompt, as [`Transcript::send_prompt_with`] says, and emits what `refused` makes of a refusal's words
    /// and the prompt it refused.
    fn send(
        &mut self,
        prompt: Prompt<FileToSend>,
        refused: fn(String, Prompt<FileToSend>) -> TranscriptEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !prompt.says_or_carries() {
            return false;
        }
        let written = Written::new(prompt);
        {
            let mut m = self.model.borrow_mut();
            for file in written.prompt.files.iter().filter(|file| file.attachment.media_type.is_image()) {
                let picture = Picture::read(file.attachment.media_type, file.data.clone());
                m.pictures.insert(file.attachment.id.clone(), picture);
            }
        }
        let sending = self.session.start_sending(&self.client, written);
        self.catch_up(cx);
        self.list.go_to_latest();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let Err(NotTaken { error, prompt }) = sending.recv().await else { return };
            let _ = this.update(cx, |t, cx| {
                t.catch_up(cx);
                cx.emit(refused(not_sent(&error), prompt));
                cx.notify();
            });
        })
        .detach();
        true
    }

    /// Asks the agent to carry on from the request it failed on, whose card's Retry goes once this is on its way. The
    /// agent already has the prompt it was working on, and what it did before the failure, so sending that again would
    /// ask it twice. A second press before the card is drawn again sends nothing more. A refusal is an error, since
    /// nobody typed what was refused, so there's nothing to give back.
    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        if self.model.borrow().retry_at.is_none() {
            return;
        }
        let refused = |message, _| TranscriptEvent::Error(message);
        self.send(Prompt::new(RETRY.to_string()), refused, cx);
    }

    /// Asks the agent to compact its conversation, keeping what `keeping` says. `typed` is what was typed to ask, if it
    /// was, which [`TranscriptEvent::NotSent`] hands back if the hub refuses, so it isn't lost. Otherwise a refusal is
    /// a [`TranscriptEvent::Error`].
    pub fn compact(&mut self, keeping: Option<String>, typed: Option<String>, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let asking = self.client.start(move |client| client.session_compact(&id, keeping.as_deref()));
        cx.spawn(async move |this, cx| {
            let Err(e) = asking.recv().await else { return };
            let message = couldnt("couldn't compact", &e);
            let _ = this.update(cx, |_, cx| match typed {
                Some(text) => cx.emit(TranscriptEvent::NotSent { message, prompt: Prompt::new(text) }),
                None => cx.emit(TranscriptEvent::Error(message)),
            });
        })
        .detach();
    }

    /// Calls off the agent's wait `wait` on its pull request, as its line's Stop does.
    pub fn call_off_wait(&mut self, wait: WaitId, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let calling_off = self.client.start(move |client| client.session_call_off_wait(&id, &wait));
        cx.spawn(async move |this, cx| {
            if let Err(e) = calling_off.recv().await {
                let message = couldnt("couldn't stop the wait", &e);
                let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(message)));
            }
        })
        .detach();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let stopping = self.client.start(move |client| client.session_cancel(&id));
        cx.spawn(async move |this, cx| {
            if let Err(e) = stopping.recv().await {
                let _ =
                    this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("couldn't stop the agent: {e}"))));
            }
        })
        .detach();
    }
}

/// What to tell someone that what they asked for didn't happen, as `what` says it. A hub older than the app refuses a
/// request it doesn't know, which updating it fixes.
fn couldnt(what: &str, error: &client_core::ClientError) -> String {
    if error.is_unsupported() {
        format!("{what}: the hub is older than the app, and can't until it's updated")
    } else {
        format!("{what}: {error}")
    }
}

/// What to tell someone whose prompt didn't go. A hub from before attachments is the one failure they can fix.
fn not_sent(error: &client_core::ClientError) -> String {
    if error.is_unsupported() {
        "couldn't send that: the hub is older than the app, and can't take attachments until it's updated".to_string()
    } else {
        format!("couldn't send that: {error}")
    }
}
