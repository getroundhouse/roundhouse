//! The queue above the composer: prompts waiting for the agent to finish, which can be changed or taken out until the hub
//! sends them. `rows` draws it.

mod rows;

use client_core::{ClientError, NotTaken, Written};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use protocol::client::ErrorCode;
use protocol::model::{ClientMessageId, QueuedPrompt, SessionId};
use protocol::prompt::says_or_carries;

use super::composer::Hub;
use super::{PaneEvent, WorkspacePane};

/// A prompt queued from here that the hub hasn't answered for yet, such as while its files are added. It shows faint at
/// the end of its chat's queue, and can't be changed until the hub has it.
pub(super) struct Pending {
    session_id: SessionId,
    prompt: QueuedPrompt,
}

/// A queued prompt being changed, in place of its text.
pub(super) struct Editing {
    session_id: SessionId,
    /// The prompt when this started, so a change can be told from none, and what it carries still shows.
    was: QueuedPrompt,
    pub(super) input: Entity<TextareaState>,
    _enter: Subscription,
}

impl WorkspacePane {
    /// Puts what's typed and attached at the end of the showing chat's queue, for the hub to send once the agent has
    /// finished. It shows at once, faint, until the hub has it, and if the hub can't take it, it goes back to its
    /// chat's composer, so nothing typed or attached is lost.
    pub fn queue_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let Some(session_id) = self.active.clone().filter(|_| self.hub == Hub::Here) else { return };
        if !says_or_carries(&text, self.attached.len()) {
            return;
        }
        let files = self.attached.take();
        self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        // At once, so what's queued can't come back as a draft.
        self.keep_typed(true, cx);
        let client_msg_id = client_core::new_message_id();
        let attachments = files.iter().map(|file| file.attachment.clone()).collect();
        let prompt = QueuedPrompt { client_msg_id: client_msg_id.clone(), text: text.clone(), attachments };
        self.queue_pending.push(Pending { session_id: session_id.clone(), prompt });
        let written = Written { client_msg_id: client_msg_id.clone(), text, files };
        let queueing = self.client.start_queueing(&session_id, written);
        cx.spawn(async move |this, cx| {
            let queued = queueing.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.queue_pending.retain(|pending| pending.prompt.client_msg_id != client_msg_id);
                match queued {
                    Ok(session) => t.took_reply(session, cx),
                    Err(NotTaken { error, text, files }) => {
                        t.give_back(Some(&session_id), text, files, cx);
                        cx.emit(PaneEvent::Error(format!("couldn't queue that: {error}")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Sends a queued prompt now: into the turn the agent is working on, which steers it, or as a turn of its own.
    pub fn send_queued_now(&mut self, client_msg_id: ClientMessageId, cx: &mut Context<Self>) {
        let Some(session_id) = self.active.clone() else { return };
        let sending = self.client.start(move |client| client.session_send_queued(&session_id, client_msg_id));
        cx.spawn(async move |this, cx| {
            let sent = sending.recv().await;
            let _ = this.update(cx, |t, cx| match sent {
                Ok(session) => t.took_reply(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't send that: {e}"))),
            });
        })
        .detach();
    }

    pub fn unqueue(&mut self, client_msg_id: ClientMessageId, cx: &mut Context<Self>) {
        let Some(session_id) = self.active.clone() else { return };
        let removing = self.client.start(move |client| client.session_unqueue(&session_id, client_msg_id));
        cx.spawn(async move |this, cx| {
            let removed = removing.recv().await;
            let _ = this.update(cx, |t, cx| match removed {
                Ok(session) => t.took_reply(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't take that out of the queue: {e}"))),
            });
        })
        .detach();
    }

    /// Shows the queued prompt's text in an editor of its own. Enter saves it and Escape leaves it as it was.
    pub fn edit_queued(&mut self, queued: &QueuedPrompt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session_id) = self.active.clone() else { return };
        let input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx).auto_grow(1, 8).submit_on_enter(true);
            state.set_value(queued.text.clone(), window, cx);
            state
        });
        input.update(cx, |i, cx| i.focus(window, cx));
        let enter = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.save_edit(window, cx);
            }
        });
        self.queue_edit = Some(Editing { session_id, was: queued.clone(), input, _enter: enter });
        cx.notify();
    }

    /// Keeps the change, unless it leaves the prompt saying nothing and carrying nothing, which keeps what was there.
    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Editing { session_id, was, input, .. }) = self.queue_edit.take() else { return };
        let text = input.read(cx).text().to_string();
        if says_or_carries(&text, was.attachments.len()) && text != was.text {
            self.change_queued(session_id, was.client_msg_id, text, cx);
        }
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Changes a queued prompt's text, unless the agent has it or it's been taken out by then.
    fn change_queued(
        &mut self,
        session_id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let changing = self.client.start(move |client| client.session_change_queued(&session_id, client_msg_id, &text));
        cx.spawn(async move |this, cx| {
            let changed = changing.recv().await;
            let _ = this.update(cx, |t, cx| match changed {
                Ok(session) => t.took_reply(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(not_changed(&e))),
            });
        })
        .detach();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_edit = None;
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Lets go of an edit whose prompt has left the queue, which the hub does when it sends it.
    pub(super) fn keep_edit_valid(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = &self.queue_edit else { return };
        let session = self.sessions.iter().find(|s| s.id == editing.session_id);
        let id = &editing.was.client_msg_id;
        let still_queued = session.is_some_and(|s| s.queued.iter().any(|queued| queued.client_msg_id == *id));
        if still_queued {
            return;
        }
        let changed = *editing.input.read(cx).text() != editing.was.text;
        self.queue_edit = None;
        if changed {
            cx.emit(PaneEvent::Error("The agent got that before the change was saved.".to_string()));
        }
    }
}

/// Why a change to a queued prompt wasn't kept, for the person who made it.
fn not_changed(error: &ClientError) -> String {
    match error {
        ClientError::Api(e) if e.code == ErrorCode::Conflict => {
            "The agent got that before the change was saved.".into()
        }
        ClientError::Api(e) if e.code == ErrorCode::NotFound => {
            "That was taken out of the queue before the change was saved.".into()
        }
        _ => format!("couldn't change that: {error}"),
    }
}
