//! One workspace's chat: the transcript the desktop app shows, and under it a composer, or what the agent asks in its
//! place.

use client_core::records::Snapshot;
use client_core::{Agents, Client, RecordsFeed};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div,
};
use look::{ButtonKind, Space};
use protocol::model::{Session, SessionId, Workspace};
use transcript::{Transcript, TranscriptEvent};

use crate::chrome::{self, TAPPED};

/// What the chat asks of the screen around it.
pub(crate) enum ChatEvent {
    Back,
}

pub(crate) struct ChatView {
    workspace: Workspace,
    client: Client,
    /// What client-core holds of the hub's records, which follows the shown chat's subagents and proposals.
    feed: RecordsFeed,
    /// What the hub says of each kind of agent, for the chat's to say what it can do.
    agents: Agents,
    shown: Option<Shown>,
    composer: Entity<TextareaState>,
    error: Option<SharedString>,
    /// What was typed of a prompt that wasn't sent, to put back in the composer, which needs a window to change.
    put_back: Option<String>,
    ensuring: Option<Task<()>>,
}

/// The chat being shown, as its record last said, and the subscription to what it says.
struct Shown {
    session: Session,
    transcript: Entity<Transcript>,
    _events: Subscription,
}

impl EventEmitter<ChatEvent> for ChatView {}

impl ChatView {
    /// Opens `session`, or with none, joins or starts the workspace's chat, on the agent the hub would start one on.
    pub(crate) fn new(
        workspace: Workspace,
        session: Option<Session>,
        feed: RecordsFeed,
        client: Client,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChatView {
        // Return makes a new line: the on-screen keyboard's Return never arrives as Enter, so sending is a button.
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).placeholder("Message the agent"));
        let agents = feed.snapshot().agents().clone();
        let mut chat = ChatView {
            workspace,
            client,
            feed,
            agents,
            shown: None,
            composer,
            error: None,
            put_back: None,
            ensuring: None,
        };
        match session {
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

    /// Takes what the hub's records now say of the workspace, such as the title it gives it, of the chat, such as
    /// whether the agent is working, and of its agents.
    pub(crate) fn records_changed(&mut self, records: &Snapshot, cx: &mut Context<Self>) {
        if let Some(workspace) = records.workspace(&self.workspace.id)
            && *workspace != self.workspace
        {
            self.workspace = workspace.clone();
            cx.notify();
        }
        if let Some(shown) = &mut self.shown {
            if let Some(session) = records.session(&shown.session.id) {
                shown.session.clone_from(session);
            }
            shown.transcript.update(cx, |transcript, cx| transcript.records_changed(records, cx));
        }
        self.set_agents(records.agents(), cx);
    }

    /// The chat it shows, once it has one.
    pub(crate) fn session(&self) -> Option<&SessionId> {
        self.shown.as_ref().map(|shown| &shown.session.id)
    }

    /// Stops following the shown chat's subagents and proposals, once the chat has closed.
    pub(crate) fn let_go(&self) {
        if let Some(session) = self.session() {
            self.feed.unfollow_session_records(session);
        }
    }

    /// What the hub says of its agents, which changes only with a new connection.
    fn set_agents(&mut self, agents: &Agents, cx: &mut Context<Self>) {
        if self.agents == *agents {
            return;
        }
        self.agents = agents.clone();
        if let Some(shown) = &self.shown {
            let agent = self.agents.of(shown.session.agent);
            shown.transcript.update(cx, |transcript, cx| transcript.set_agent(agent, cx));
        }
    }

    /// What its header names it: the workspace, as its record last did.
    pub(crate) fn title(&self) -> &str {
        self.workspace.label()
    }

    #[cfg(feature = "testing")]
    pub(crate) fn transcript(&self) -> Option<Entity<Transcript>> {
        self.shown.as_ref().map(|shown| shown.transcript.clone())
    }

    /// Another client, or whoever made this workspace, may be about to give it a chat, so this joins that rather than
    /// making a second.
    fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        let (id, agent) = (self.workspace.id.clone(), self.agents.for_new_chat());
        let found = self.client.start(move |client| client.session_ensure(&id, agent));
        self.ensuring = Some(cx.spawn(async move |this, cx| {
            let found = found.recv().await;
            let _ = this.update(cx, |chat, cx| {
                chat.ensuring = None;
                match found {
                    Ok(session) => chat.show(session, cx),
                    Err(e) => chat.error = Some(format!("Couldn't open a chat: {e}").into()),
                }
                cx.notify();
            });
        }));
    }

    fn show(&mut self, session: Session, cx: &mut Context<Self>) {
        // Its subagents' and proposals' calls are drawn from their records, which client-core keeps once a chat asks
        // for them, and the transcript takes as they come.
        self.feed.follow_session_records(&session.id);
        let (client, records) = (self.client.clone(), self.feed.snapshot());
        let agent = self.agents.of(session.agent);
        let transcript = cx.new(|cx| {
            let mut transcript = Transcript::new(client, &session, cx);
            transcript.set_agent(agent, cx);
            transcript.records_changed(&records, cx);
            transcript
        });
        let events = cx.subscribe(&transcript, |chat, _, event: &TranscriptEvent, cx| chat.heard(event, cx));
        self.shown = Some(Shown { session, transcript, _events: events });
        cx.notify();
    }

    fn heard(&mut self, event: &TranscriptEvent, cx: &mut Context<Self>) {
        match event {
            TranscriptEvent::Error(message) => self.error = Some(message.clone().into()),
            // What was typed comes back when nothing new has been typed since, so it can be sent again.
            TranscriptEvent::NotSent { message, prompt } => {
                self.error = Some(message.clone().into());
                self.put_back = (self.composer.read(cx).value().is_empty()).then(|| prompt.text.clone());
            }
            // Drawn in the composer's place, or gone from it.
            TranscriptEvent::Asking { .. } => {}
            TranscriptEvent::TurnEnded
            | TranscriptEvent::Answering { .. }
            | TranscriptEvent::OpenSubagent { .. }
            | TranscriptEvent::OpenProposal { .. }
            | TranscriptEvent::FindClosed => return,
        }
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Shown { transcript, .. }) = &self.shown else { return };
        let text = self.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        if transcript.update(cx, |transcript, cx| transcript.send_prompt(text, cx)) {
            self.error = None;
            self.composer.update(cx, |composer, cx| composer.set_value("", window, cx));
        }
    }

    /// What's typed and the button that sends it, or while the agent asks something, that in their place, as on the
    /// desktop.
    fn composer_row(&self, bottom: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let row = div().w_full().flex_none().px(Space::Medium).pt(Space::Small).pb(bottom + Space::Small.size());
        if let Some(question) = self.shown.as_ref().and_then(|shown| shown.transcript.read(cx).question().cloned()) {
            return row.child(question);
        }
        let send = look::icon_button("send", ButtonKind::Main, TAPPED, IconName::ArrowUp)
            .accessibility_label("Send")
            .off(self.shown.is_none())
            .on_click(cx.listener(|chat, _, window, cx| chat.send(window, cx)));
        row.border_t_1().border_color(look::colors(cx).edge).child(
            h_flex()
                .items_end()
                .gap(Space::Small)
                .child(
                    div()
                        .id("composer")
                        .flex_1()
                        .min_w_0()
                        .child(Textarea::new(&self.composer).aria_label("Message"))
                        .test_support(),
                )
                .child(send),
        )
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(text) = self.put_back.take() {
            self.composer.update(cx, |composer, cx| composer.set_value(text, window, cx));
        }
        let back = chrome::back().on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Back)));
        let title = SharedString::from(self.title().to_string());
        let branch = chrome::branch_under_name(&self.workspace);
        let body: AnyElement = match &self.shown {
            Some(shown) => div().flex_1().min_h_0().w_full().child(shown.transcript.clone()).into_any_element(),
            None => chrome::empty("Opening the chat", ""),
        };
        v_flex()
            .size_full()
            .child(chrome::header(title, branch, Some(back), Vec::new(), window, cx))
            .when_some(self.error.clone(), |d, error| d.child(chrome::error(error)))
            .child(body)
            .child(self.composer_row(window.insets().effective().bottom, cx))
    }
}
