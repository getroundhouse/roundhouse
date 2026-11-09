//! One workspace's chat: the transcript the desktop app shows, and a composer under it.

use client_core::Client;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, TestSupportExt as _, Window, div, px,
};
use protocol::model::{AgentKind, Session, Workspace};
use transcript::{Transcript, TranscriptEvent};

use crate::chrome::{self, TAP_HEIGHT};

/// What the chat asks of the screen around it.
pub(crate) enum ChatEvent {
    Back,
}

pub(crate) struct ChatView {
    workspace: Workspace,
    client: Client,
    shown: Option<Shown>,
    composer: Entity<TextareaState>,
    error: Option<SharedString>,
    ensuring: Option<Task<()>>,
}

/// The chat being shown, and the subscription to its errors.
struct Shown {
    transcript: Entity<Transcript>,
    _errors: Subscription,
}

impl EventEmitter<ChatEvent> for ChatView {}

impl ChatView {
    /// Opens the workspace's most recently active chat, or joins or starts one if it has none open.
    pub(crate) fn new(
        workspace: Workspace,
        sessions: &[Session],
        client: Client,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChatView {
        // Return makes a new line: the on-screen keyboard's Return never arrives as Enter, so sending is a button.
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).placeholder("Message the agent"));
        let mut chat = ChatView { workspace, client, shown: None, composer, error: None, ensuring: None };
        match chrome::chat_to_open(sessions) {
            Some(session) => chat.show(session, cx),
            None => chat.ensure_chat(cx),
        }
        chat
    }

    /// Carries on over a new connection, keeping what's shown and what's typed.
    pub(crate) fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        if let Some(shown) = &self.shown {
            shown.transcript.update(cx, |transcript, _| transcript.reconnect(client));
        } else {
            // Asked again on the new connection, since one asked on the old would fail with it.
            self.error = None;
            self.ensure_chat(cx);
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn transcript(&self) -> Option<Entity<Transcript>> {
        self.shown.as_ref().map(|shown| shown.transcript.clone())
    }

    /// Another client, or whoever made this workspace, may be about to give it a chat, so this joins that
    /// rather than making a second.
    fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        self.ensuring = Some(cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { client.session_ensure(&id, AgentKind::Claude) }).await;
            let _ = this.update(cx, |chat, cx| {
                chat.ensuring = None;
                match found {
                    Ok(session) => chat.show(&session, cx),
                    Err(e) => chat.error = Some(format!("Couldn't open a chat: {e}").into()),
                }
                cx.notify();
            });
        }));
    }

    fn show(&mut self, session: &Session, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let transcript = cx.new(|cx| Transcript::new(client, session, cx));
        let errors = cx.subscribe(&transcript, |chat, _, event: &TranscriptEvent, cx| {
            if let TranscriptEvent::Error(message) = event {
                chat.error = Some(message.clone().into());
                cx.notify();
            }
        });
        self.shown = Some(Shown { transcript, _errors: errors });
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Shown { transcript, .. }) = &self.shown else { return };
        let text = self.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        // While the agent waits on a question, what's typed is the answer to it.
        let taken = transcript
            .update(cx, |transcript, cx| transcript.answer_with_text(&text, cx) || transcript.send_prompt(text, cx));
        if taken {
            self.error = None;
            self.composer.update(cx, |composer, cx| composer.set_value("", window, cx));
        }
    }

    fn composer_row(&self, bottom: gpui_kit::Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .flex_shrink_0()
            .items_end()
            .gap_2()
            .px_3()
            .pt_2()
            .pb(bottom + px(8.))
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("composer")
                    .flex_1()
                    .min_w_0()
                    .child(Textarea::new(&self.composer).aria_label("Message"))
                    .test_support(),
            )
            .child(
                Button::new("send")
                    .icon(IconName::ArrowUp)
                    .primary()
                    .w(px(TAP_HEIGHT))
                    .h(px(TAP_HEIGHT))
                    .disabled(self.shown.is_none())
                    .on_click(cx.listener(|chat, _, window, cx| chat.send(window, cx))),
            )
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        let back = Button::new("back")
            .icon(IconName::ChevronLeft)
            .ghost()
            .w(px(TAP_HEIGHT))
            .h(px(TAP_HEIGHT))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Back)));
        let title = SharedString::from(self.workspace.name.clone());
        let branch = chrome::branch_under_name(&self.workspace);
        v_flex()
            .size_full()
            .child(chrome::header(title, branch, Some(back.into_any_element()), None, insets.safe_area.top, cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().px_3().py_2().text_sm().text_color(cx.theme().danger).child(error))
            })
            .child(match &self.shown {
                Some(shown) => div().flex_1().min_h_0().w_full().child(shown.transcript.clone()).into_any_element(),
                None => chrome::empty("Opening the chat", "", cx).into_any_element(),
            })
            .child(self.composer_row(insets.effective().bottom, cx))
    }
}
