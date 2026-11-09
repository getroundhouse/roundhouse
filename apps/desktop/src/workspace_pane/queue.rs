//! The queue above the composer: prompts waiting for the agent to finish, which can be changed or taken out until the hub
//! sends them.

use client_core::ClientError;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, InputEvent, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, Entity, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _, Window, div, px,
};
use protocol::client::ErrorCode;
use protocol::model::{ClientMessageId, QueuedPrompt, SessionId};

use super::{PaneEvent, WorkspacePane};

/// About five prompts of a line each.
const QUEUE_MOST_HEIGHT: f32 = 200.;

/// A queued prompt being changed, in place of its text.
pub(super) struct Editing {
    session_id: SessionId,
    client_msg_id: ClientMessageId,
    /// What it said when this started, so a change can be told from none.
    was: String,
    pub(super) input: Entity<TextareaState>,
    _enter: Subscription,
}

impl WorkspacePane {
    /// Puts what's typed at the end of the showing chat's queue, for the hub to send once the agent has finished. A
    /// queued prompt can't carry files yet, so one with files stays in the composer, to send now.
    pub fn queue_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let Some(session_id) = self.active.clone() else { return };
        if !self.attached.is_empty() {
            cx.emit(PaneEvent::Error("A prompt with files can't be queued yet. Enter sends it now.".to_string()));
            return;
        }
        if text.trim().is_empty() {
            return;
        }
        self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        self.save_queued(session_id, client_core::new_message_id(), text, true, cx);
    }

    /// Queues `text` under `client_msg_id`, or changes the queued prompt with that ID. If it can't, text that came from the
    /// composer goes back to its chat's, so nothing typed is lost.
    fn save_queued(
        &mut self,
        session_id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
        from_composer: bool,
        cx: &mut Context<Self>,
    ) {
        let (sent, to) = (text.clone(), session_id.clone());
        let saving = self.client.start(move |client| client.session_queue(&to, client_msg_id, &sent));
        cx.spawn(async move |this, cx| {
            let saved = saving.recv().await;
            let _ = this.update(cx, |t, cx| match saved {
                Ok(session) => t.took_reply(session, cx),
                Err(ClientError::Api(e)) if e.code == ErrorCode::Conflict => {
                    cx.emit(PaneEvent::Error("The agent got that before the change was saved.".to_string()));
                }
                Err(e) => {
                    if from_composer {
                        t.give_back(Some(&session_id), text, Vec::new(), cx);
                    }
                    cx.emit(PaneEvent::Error(format!("couldn't queue that: {e}")));
                }
            });
        })
        .detach();
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
        self.queue_edit = Some(Editing {
            session_id,
            client_msg_id: queued.client_msg_id.clone(),
            was: queued.text.clone(),
            input,
            _enter: enter,
        });
        cx.notify();
    }

    /// Keeps the change, unless it leaves nothing, which keeps what was there.
    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.queue_edit.take() else { return };
        let text = editing.input.read(cx).text().to_string();
        if !text.trim().is_empty() && text != editing.was {
            self.save_queued(editing.session_id, editing.client_msg_id, text, false, cx);
        }
        self.focus_composer(window, cx);
        cx.notify();
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
        let still_queued =
            session.is_some_and(|s| s.queued.iter().any(|queued| queued.client_msg_id == editing.client_msg_id));
        if still_queued {
            return;
        }
        let changed = *editing.input.read(cx).text() != editing.was;
        self.queue_edit = None;
        if changed {
            cx.emit(PaneEvent::Error("The agent got that before the change was saved.".to_string()));
        }
    }

    /// The showing chat's queue, oldest first, or nothing when it's empty. A held queue says so.
    pub(super) fn queue_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let (queued, held, working) = (session.queued.clone(), session.queue_held, session.status.in_turn());
        if queued.is_empty() {
            return None;
        }
        let rows: Vec<AnyElement> = queued.iter().map(|prompt| self.queued_row(prompt, working, cx)).collect();
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let heading = if held { "Queued, waiting since the agent stopped" } else { "Queued" };
        Some(
            v_flex()
                .id("queue")
                .mx_3()
                .rounded_t_lg()
                .border_1()
                .border_b_0()
                .border_color(border)
                .bg(theme.background)
                .child(
                    div().px_3().py_1p5().border_b_1().border_color(border).text_xs().text_color(muted).child(heading),
                )
                // Its own height, so a long queue scrolls rather than pushing the composer out of the window.
                .child(div().id("queue-rows").max_h(px(QUEUE_MOST_HEIGHT)).overflow_y_scroll().p_1().children(rows))
                .test_support()
                .into_any_element(),
        )
    }

    /// One line of the prompt, and under the pointer, a lit row with buttons to change it, take it out, and send it now.
    /// While the agent works, sending it steers the agent with it, and otherwise it's a turn of its own.
    fn queued_row(&self, prompt: &QueuedPrompt, working: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = prompt.client_msg_id.clone();
        let editing = self.queue_edit.as_ref().filter(|editing| editing.client_msg_id == id);
        if let Some(editing) = editing {
            return div()
                .id(ElementId::Name(format!("queued-editing-{id}").into()))
                .px_1()
                .py_1()
                .on_action(cx.listener(|t, _: &Escape, window, cx| t.cancel_edit(window, cx)))
                .child(Textarea::new(&editing.input).appearance(false))
                .into_any_element();
        }
        let group = SharedString::from(format!("queued-{id}"));
        let theme = cx.theme();
        // Muted, so they don't compete with the prompt's text, and the row a step lighter than the card under them.
        let (muted, lit) = (theme.muted_foreground, theme.muted);
        let (to_edit, to_remove, to_send) = (prompt.clone(), id.clone(), id.clone());
        let button = |name: &str, icon: IconName, tooltip: &'static str| {
            Button::new(ElementId::Name(format!("{name}-queued-{id}").into()))
                .ghost()
                .xsmall()
                .icon(icon)
                .text_color(muted)
                .tooltip(tooltip)
        };
        let edit = button("edit", IconName::Pencil, "Edit")
            .on_click(cx.listener(move |t, _, window, cx| t.edit_queued(&to_edit, window, cx)));
        let remove = button("remove", IconName::Trash, "Remove")
            .on_click(cx.listener(move |t, _, _, cx| t.unqueue(to_remove.clone(), cx)));
        let send = button("send", IconName::ArrowUp, if working { "Steer" } else { "Send now" })
            .on_click(cx.listener(move |t, _, _, cx| t.send_queued_now(to_send.clone(), cx)));
        // Edit shows all of it.
        let line = prompt.text.split_whitespace().collect::<Vec<_>>().join(" ");
        h_flex()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded_md()
            .hover(move |style| style.bg(lit))
            .child(div().flex_1().min_w_0().text_sm().truncate().child(line))
            // Hidden rather than left out, so the row keeps its height when they show.
            .child(
                h_flex()
                    .flex_none()
                    .gap_0p5()
                    .invisible()
                    .group_hover(group, Styled::visible)
                    .child(edit)
                    .child(remove)
                    .child(send),
            )
            .test_support()
            .into_any_element()
    }
}
