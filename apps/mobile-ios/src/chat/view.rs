//! The chat's own screen: its transcript, and the composer under it.

use client_core::records::Snapshot;
use client_core::{Agents, Client, Hub, RecordsFeed};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div,
};
use look::{ButtonKind, Lined as _, Space};
use protocol::model::{Session, SessionId, Workspace, WorkspaceId};
use transcript::{Transcript, TranscriptEvent};

use super::chats::chats_menu;
use super::draft::Kept;
use super::over::Over;
use super::reading::ChatReading;
use crate::chrome::{self, TAPPED};
use crate::hub_link::Ready;

/// How far apart the lines of what's read in the chat are, from one's top to the next's. The transcript takes it from
/// what it's drawn in, where a desk's comes to 1.6 times its text's size. Here that would be 31 points, by the
/// handheld's rem rather than its body text, which shows too little of a reply at once, so it's set for 17-point
/// text: a little more than iOS's own 22, since a reply is read at length. *Not built:* the transcript taking its
/// sizes from the foundations, as `docs/spec/look.md` says, which would set this for every handheld.
pub(super) const PROSE_LINE: Pixels = gpui_kit::px(25.);

/// What the chat asks of the screen around it.
pub(crate) enum ChatEvent {
    Back,
    /// Show this workspace's chat instead, as the menu of a pull request the chat names asks.
    GoToWorkspace(WorkspaceId),
}

pub(crate) struct ChatView {
    pub(super) workspace: Workspace,
    /// What the chat, and what's open over it, are opened from, and what they ask the hub through.
    pub(super) hub: Hub,
    pub(super) client: Client,
    /// What client-core holds of the hub's records, which follows the shown chat's subagents and proposals.
    pub(super) feed: RecordsFeed,
    /// What the hub says of each kind of agent, for the chat's to say what it can do.
    pub(super) agents: Agents,
    pub(super) shown: Option<Shown>,
    /// What a line in the chat opened over it.
    pub(super) over: Over,
    pub(super) composer: Entity<TextareaState>,
    /// What has the hub keep what's typed in it.
    pub(super) kept: Kept,
    /// Whether the chat shown is being read.
    pub(super) read: ChatReading,
    /// Which list of models the picker offers, for an agent that discovers its models.
    pub(super) models: client_ui::FollowedModels,
    pub(super) error: Option<SharedString>,
    /// What to put in the composer, which needs a window to change: a draft the hub kept, or what was typed of a
    /// prompt that wasn't sent.
    pub(super) put_back: Option<String>,
    /// A chat being found, started or reopened, to show once the hub says it is.
    pub(super) opening: Option<Task<()>>,
    /// Whether anything's typed, which decides between Send and Stop.
    _typing: Subscription,
}

/// The chat being shown, as its record last said, and the subscription to what it says.
pub(super) struct Shown {
    pub(super) session: Session,
    pub(super) transcript: Entity<Transcript>,
    _events: Subscription,
}

impl EventEmitter<ChatEvent> for ChatView {}

impl ChatView {
    /// Opens `session`, or with none, joins or starts the workspace's chat, on the agent the hub would start one on.
    pub(crate) fn new(
        workspace: Workspace,
        session: Option<Session>,
        hub: Ready,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChatView {
        // Return makes a new line: the on-screen keyboard's Return never arrives as Enter, so sending is a button.
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).placeholder("Message the agent"));
        let typing = cx.subscribe(&composer, |chat, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                chat.keep_typed(false, cx);
                cx.notify();
            }
        });
        let Ready { hub, client, feed, drafts } = hub;
        let agents = feed.snapshot().agents().clone();
        let mut chat = ChatView {
            workspace,
            hub,
            client,
            feed,
            agents,
            shown: None,
            over: Over::default(),
            composer,
            kept: Kept::new(drafts),
            read: ChatReading::new(window),
            models: client_ui::FollowedModels::default(),
            error: None,
            put_back: None,
            opening: None,
            _typing: typing,
        };
        match session {
            Some(session) => chat.show(session, cx),
            None => chat.ensure_chat(cx),
        }
        chat
    }

    /// Carries on over a new connection, keeping what's shown and what's typed. The transcript, and what's open over
    /// it, carry on by themselves, from the hub they were opened from.
    pub(crate) fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        if self.shown.is_some() {
            // The new connection's hub may hold other lists.
            self.models.reset();
            self.follow_models(false, cx);
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
        self.take_draft_from_elsewhere(cx);
        self.read_showing(cx);
        self.follow_models(false, cx);
        self.over_records_changed(records, cx);
        self.set_agents(records.agents(), cx);
    }

    /// The chat it shows, once it has one.
    pub(crate) fn session(&self) -> Option<&SessionId> {
        self.shown.as_ref().map(|shown| &shown.session.id)
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

    /// The workspace whose chat it is.
    pub(crate) fn workspace(&self) -> &WorkspaceId {
        &self.workspace.id
    }

    /// What its header names it: the workspace, as its record last did.
    pub(crate) fn title(&self) -> &str {
        self.workspace.label()
    }

    /// What it says went wrong, while it says it.
    #[cfg(feature = "testing")]
    pub(crate) fn said(&self) -> Option<String> {
        self.error.as_ref().map(ToString::to_string)
    }

    #[cfg(feature = "testing")]
    pub(crate) fn typed(&self, cx: &App) -> String {
        self.composer.read(cx).value().to_string()
    }

    /// The phone's record of the chat it shows, which it decides whether it's read from.
    #[cfg(feature = "testing")]
    pub(crate) fn shown_record(&self) -> Option<&Session> {
        self.shown.as_ref().map(|shown| &shown.session)
    }

    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn transcript(&self) -> Option<Entity<Transcript>> {
        self.shown.as_ref().map(|shown| shown.transcript.clone())
    }

    /// Another client, or whoever made this workspace, may be about to give it a chat, so this joins that rather than
    /// making a second.
    fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        let (id, agent) = (self.workspace.id.clone(), self.agents.for_new_chat());
        let found = self.client.start(move |client| client.session_ensure(&id, agent));
        self.opening = Some(cx.spawn(async move |this, cx| {
            let found = found.recv().await;
            let _ = this.update(cx, |chat, cx| {
                chat.opening = None;
                match found {
                    Ok(session) => chat.show(session, cx),
                    Err(e) => chat.error = Some(format!("Couldn't open a chat: {e}").into()),
                }
                cx.notify();
            });
        }));
    }

    /// Shows `session`, with what the hub kept of its composer, but for the first chat shown, which keeps what was
    /// typed while it was being found, if the hub has nothing for it.
    pub(super) fn show(&mut self, session: Session, cx: &mut Context<Self>) {
        let typed = !self.composer.read(cx).value().trim().is_empty();
        let keep_typed = self.shown.is_none() && typed && session.draft.text.is_empty();
        if !keep_typed {
            self.take_draft(&session);
        }
        let (hub, records, feed) = (self.hub.clone(), self.feed.snapshot(), self.feed.clone());
        let agent = self.agents.of(session.agent);
        let transcript = cx.new(|cx| {
            let mut transcript = Transcript::new(hub, &feed, &session, cx);
            transcript.set_agent(agent, cx);
            transcript.records_changed(&records, cx);
            transcript
        });
        let events = cx.subscribe(&transcript, |chat, _, event: &TranscriptEvent, cx| chat.heard(event, cx));
        self.shown = Some(Shown { session, transcript, _events: events });
        if keep_typed {
            self.keep_typed(true, cx);
        }
        self.read_showing(cx);
        self.models.reset();
        self.follow_models(false, cx);
        cx.notify();
    }

    /// Says what went wrong, across the top of whichever screen is in front.
    pub(super) fn say(&mut self, message: &str, cx: &mut Context<Self>) {
        self.error = Some(message.to_string().into());
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
            TranscriptEvent::GoToWorkspace { workspace } => {
                cx.emit(ChatEvent::GoToWorkspace(workspace.clone()));
                return;
            }
            TranscriptEvent::OpenSubagent { subagent, title } => {
                self.open_subagent(subagent.clone(), title.clone(), cx);
            }
            TranscriptEvent::OpenProposal { proposal } => self.open_proposal(proposal, cx),
            TranscriptEvent::TurnEnded | TranscriptEvent::Answering { .. } | TranscriptEvent::FindClosed => return,
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
            self.keep_typed(true, cx);
        }
    }

    /// What's typed and the button that sends it, or while the agent asks something, that in their place, as on the
    /// desktop.
    fn composer_row(&self, bottom: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let row = div().w_full().flex_none().px(Space::Medium).pt(Space::Small).pb(bottom + Space::Small.size());
        if let Some(question) = self.shown.as_ref().and_then(|shown| shown.transcript.read(cx).question().cloned()) {
            return row.child(question);
        }
        let actions = self.actions(cx);
        let settings = self.settings_row(cx);
        let typed = h_flex()
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
            .children(actions);
        // What the chat runs with goes under what's typed, as on the desktop.
        let settings = (!settings.is_empty()).then(|| h_flex().items_center().gap(Space::XSmall).children(settings));
        row.line_over(cx).child(v_flex().gap(Space::XSmall).child(typed).children(settings))
    }
}

impl ChatView {
    /// Send, and while the agent is in a turn, Stop at the end for the whole of it, as on the desktop, with Send before
    /// it only once something's typed, which steers the agent with it, as the desktop's Enter does, since a phone's
    /// Return only types a new line. So Stop doesn't move during a turn: once Send has sent, what's typed takes its
    /// room, and a second tap lands there rather than on Stop, as the owner decided. Once the turn ends, Send takes the
    /// end again.
    fn actions(&self, cx: &mut Context<Self>) -> Vec<look::Button> {
        let working = self.shown.as_ref().is_some_and(|shown| shown.session.status.in_turn());
        let send = || {
            look::icon_button("send", ButtonKind::Main, TAPPED, IconName::ArrowUp)
                .accessibility_label("Send")
                .off(self.shown.is_none())
                .on_click(cx.listener(|chat, _, window, cx| chat.send(window, cx)))
        };
        if !working {
            return vec![send()];
        }
        let typed = !self.composer.read(cx).value().trim().is_empty();
        let stop = look::stop_button("stop", TAPPED, cx).on_click(cx.listener(|chat, _, _, cx| chat.stop(cx)));
        if typed { vec![send(), stop] } else { vec![stop] }
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(shown) = &self.shown {
            shown.transcript.update(cx, Transcript::cancel);
        }
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(text) = self.put_back.take() {
            self.composer.update(cx, |composer, cx| composer.set_value(text, window, cx));
        }
        if let Some(over) = self.over_screen(window, cx) {
            return over;
        }
        let back = chrome::back().on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Back)));
        let title = SharedString::from(self.title().to_string());
        let branch = chrome::branch_under_name(&self.workspace);
        let body: AnyElement = match &self.shown {
            Some(shown) => read(&shown.transcript, cx),
            None => chrome::empty("Opening the chat", ""),
        };
        v_flex()
            .size_full()
            .child(
                chrome::header(title, branch, Some(back), Vec::new(), window, cx)
                    .when(self.shown.is_some(), |bar| bar.menu("chats", chats_menu(cx.weak_entity()))),
            )
            .when_some(self.error.clone(), |d, error| d.child(chrome::error(error)))
            .child(body)
            .child(self.composer_row(crate::safe_area::insets(window, cx).effective().bottom, cx))
            .into_any_element()
    }
}

/// A transcript to read, the chat's or a subagent's, with its lines set apart for reading at length.
pub(super) fn read(transcript: &Entity<Transcript>, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .w_full()
        .line_height(PROSE_LINE)
        .child(Transcript::drawn(transcript, cx))
        .into_any_element()
}
