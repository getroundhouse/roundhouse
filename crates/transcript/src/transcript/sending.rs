//! What's sent from a chat: its prompts, with what they carry, and Stop.

use client_core::FileToSend;
use gpui_kit::Context;

use super::{Transcript, TranscriptEvent};
use crate::attachments::Picture;

impl Transcript {
    /// Shows the prompt at once, and takes it back if it can't be sent. Returns false if it's empty.
    pub fn send_prompt(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        self.send_prompt_with(text, Vec::new(), cx)
    }

    /// Shows the prompt at once with what it carries, then adds each file to the session and sends the prompt. If any
    /// of that fails, it takes the prompt back and hands its text and files back with [`TranscriptEvent::NotSent`].
    /// Returns false if there's nothing to send.
    pub fn send_prompt_with(&mut self, text: String, files: Vec<FileToSend>, cx: &mut Context<Self>) -> bool {
        if text.trim().is_empty() && files.is_empty() {
            return false;
        }
        let client_msg_id = client_core::new_message_id();
        {
            let mut m = self.model.borrow_mut();
            for file in files.iter().filter(|file| file.attachment.media_type.is_image()) {
                let picture = Picture::read(file.attachment.media_type, file.data.clone());
                m.pictures.insert(file.attachment.id.clone(), picture);
            }
        }
        let carried = files.iter().map(|file| file.attachment.clone()).collect();
        self.session.add_pending(client_msg_id.clone(), text.clone(), carried);
        self.catch_up(cx);
        self.list.go_to_latest();
        cx.notify();

        // Started on a thread of its own, so it finishes even if the app quits, and hands back what it was given.
        let (id, msg_id, typed) = (self.session_id.clone(), client_msg_id.clone(), text.clone());
        let sending = self.client.start(move |client| {
            let sent = client.send_prompt_carrying(&id, &text, Some(msg_id), &files);
            Ok((sent, text, files))
        });
        cx.spawn(async move |this, cx| {
            let (message, text, files) = match sending.recv().await {
                Ok((Ok(_), ..)) => return,
                Ok((Err(e), text, files)) => (not_sent(&e), text, files),
                // Only a call that never ran ends here, and its files went with it.
                Err(e) => (not_sent(&e), typed, Vec::new()),
            };
            let _ = this.update(cx, |t, cx| {
                t.session.remove_pending(&client_msg_id);
                t.catch_up(cx);
                cx.emit(TranscriptEvent::NotSent { message, text, files });
                cx.notify();
            });
        })
        .detach();
        true
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

/// What to tell someone whose prompt didn't go. A hub from before attachments is the one failure they can fix.
fn not_sent(error: &client_core::ClientError) -> String {
    if error.is_unsupported() {
        "couldn't send that: the hub is older than the app, and can't take attachments until it's updated".to_string()
    } else {
        format!("couldn't send that: {error}")
    }
}
