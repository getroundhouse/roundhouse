//! One workspace: its chats, the composer, and the diff beside them when it's open.

use std::collections::HashMap;

use client_core::Client;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, AnyWindowHandle, AppContext, Context, ElementId, Entity, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, ParentElement, PromptLevel, Render, Result, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, Window, div, px,
};
use protocol::model::{AgentKind, PullRequest, Session, SessionId, SessionStatus, SetupStatus, Workspace, WorkspaceId};
use transcript::{Transcript, TranscriptEvent, mode_label};

use crate::diff_panel::{DiffEvent, DiffPanel};

const SETUP_LOG_REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// Aliases, so these stay the newest of each without a release here. `default` is the account's own.
const MODELS: [(&str, &str); 5] =
    [("default", "Default model"), ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];
const MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "bypassPermissions"];

fn model_label(model: Option<&str>) -> String {
    let model = model.unwrap_or("default");
    MODELS.iter().find(|(id, _)| *id == model).map_or(model, |(_, label)| label).to_string()
}

fn chat_title(session: &Session, number: usize) -> String {
    session.title.clone().unwrap_or_else(|| format!("Chat {number}"))
}

fn is_working(session: &Session) -> bool {
    matches!(session.status, SessionStatus::Running | SessionStatus::WaitingApproval)
}

/// What a new chat starts with. It keeps the model and how edits are handled, but Plan belongs to the
/// task at hand, and full access should be chosen on purpose each time.
fn inherited_mode(mode: Option<&str>) -> Option<String> {
    mode.filter(|mode| !matches!(*mode, "plan" | "bypassPermissions")).map(String::from)
}

/// The chat after this one, or else the one before.
fn neighbor(open: &[SessionId], position: usize) -> Option<SessionId> {
    open.get(position).or_else(|| open.last()).cloned()
}

#[derive(Clone, Debug)]
pub enum PaneEvent {
    Error(String),
    Disconnected,
    Archive(WorkspaceId, bool),
}

/// What `Create PR` asks of the agent. The agent writes it, because it knows why the changes were made.
fn pull_request_prompt(workspace: &Workspace) -> String {
    let base = workspace.base_ref.as_deref().unwrap_or("the default branch");
    let base = base.strip_prefix("origin/").unwrap_or(base);
    format!(
        "Create a pull request for this branch. Commit any uncommitted work that belongs in it, push the branch, \
         and open the PR against `{base}` with `gh pr create`. Write the title and description from the diff and \
         what we did here. Reply with the PR's URL."
    )
}

pub struct WorkspacePane {
    client: Client,
    pub workspace: Workspace,
    /// In the order they were made, including closed ones.
    pub sessions: Vec<Session>,
    pub active: Option<SessionId>,
    /// Made the first time a chat is shown, then kept so it stays current and keeps its place.
    transcripts: HashMap<SessionId, (Entity<Transcript>, Subscription)>,
    /// What's been typed in the chats not showing.
    drafts: HashMap<SessionId, String>,
    /// A new chat is on its way, so another press doesn't make a second.
    creating_chat: bool,
    pub composer: Entity<TextareaState>,
    pub diff: Option<Entity<DiffPanel>>,
    pub diff_open: bool,
    pub pull_request: Option<PullRequest>,
    pub setup_log: Option<String>,
    /// Keeps the open log current while setup runs.
    setup_log_refresh: Option<Task<()>>,
    window: AnyWindowHandle,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaneEvent> for WorkspacePane {}

impl WorkspacePane {
    pub fn new(
        client: Client,
        workspace: Workspace,
        sessions: Vec<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 12)
                .submit_on_enter(true)
                .placeholder("Ask the agent to do something")
        });
        let on_enter = cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.submit(window, cx);
            }
        });
        let mut this = Self {
            client,
            workspace,
            sessions,
            active: None,
            transcripts: HashMap::new(),
            drafts: HashMap::new(),
            creating_chat: false,
            composer,
            diff: None,
            diff_open: false,
            pull_request: None,
            setup_log: None,
            setup_log_refresh: None,
            window: window.window_handle(),
            subscriptions: vec![on_enter],
        };
        // The chat last worked in.
        let latest = this.open_sessions().max_by_key(|s| (s.updated_at_ms, s.created_at_ms)).map(|s| s.id.clone());
        match latest {
            Some(id) => this.activate(id, cx),
            None => this.ensure_chat(cx),
        }
        this.refresh_pull_request(cx);
        this
    }

    fn open_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.iter().filter(|s| s.closed_at_ms.is_none())
    }

    pub fn session(&self) -> Option<&Session> {
        self.active.as_ref().and_then(|id| self.sessions.iter().find(|s| &s.id == id))
    }

    pub fn transcript(&self) -> Option<Entity<Transcript>> {
        self.active.as_ref().and_then(|id| self.transcripts.get(id)).map(|(view, _)| view.clone())
    }

    /// For a workspace with no open chat. Another client, or whoever is making this workspace, may be
    /// about to give it one, so this joins that rather than making a second.
    fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_chat {
            return;
        }
        self.creating_chat = true;
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { client.session_ensure(&id, AgentKind::Claude) }).await;
            let _ = this.update(cx, |t, cx| t.chat_created(found, cx));
        })
        .detach();
    }

    /// Starts another chat in this workspace and shows it.
    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_chat {
            return;
        }
        self.creating_chat = true;
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        let model = self.session().and_then(|s| s.model.clone());
        let mode = inherited_mode(self.session().and_then(|s| s.permission_mode.as_deref()));
        cx.spawn(async move |this, cx| {
            let created =
                cx.background_spawn(async move { client.session_create(&id, AgentKind::Claude, model, mode) }).await;
            let _ = this.update(cx, |t, cx| t.chat_created(created, cx));
        })
        .detach();
    }

    fn chat_created(&mut self, created: Result<Session, client_core::ClientError>, cx: &mut Context<Self>) {
        self.creating_chat = false;
        match created {
            Ok(session) => {
                let id = session.id.clone();
                self.set_session(session, cx);
                self.activate(id, cx);
            }
            Err(e) => cx.emit(PaneEvent::Error(format!("couldn't start a chat: {e}"))),
        }
    }

    /// Shows a chat, keeping what was being typed in the one it replaces.
    pub fn activate(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if self.active.as_ref() == Some(&id) {
            return;
        }
        if let Some(current) = self.active.take() {
            let draft = self.composer.read(cx).text().to_string();
            if draft.is_empty() {
                self.drafts.remove(&current);
            } else {
                self.drafts.insert(current, draft);
            }
        }
        if !self.transcripts.contains_key(&id)
            && let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned()
        {
            self.make_transcript(session, cx);
        }
        self.active = Some(id);
        self.show_draft(cx);
        cx.notify();
    }

    /// This needs the window, and a click handler is already inside a window update, so it runs just
    /// after whatever called it.
    fn show_draft(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| {
                    let draft = t.active.as_ref().and_then(|id| t.drafts.remove(id)).unwrap_or_default();
                    let end = draft.len();
                    t.composer.update(cx, |c, cx| {
                        c.set_value(draft, window, cx);
                        c.set_selected_range(end..end, cx);
                        c.focus(window, cx);
                    });
                });
            });
        });
    }

    fn make_transcript(&mut self, session: Session, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let view = cx.new(|cx| Transcript::new(client, &session, cx));
        let events = cx.subscribe(&view, |this, _, event: &TranscriptEvent, cx| match event {
            TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            TranscriptEvent::Disconnected => cx.emit(PaneEvent::Disconnected),
            TranscriptEvent::TurnEnded => {
                this.refresh_pull_request(cx);
                if let Some(diff) = &this.diff {
                    diff.update(cx, DiffPanel::refresh);
                }
            }
        });
        self.transcripts.insert(session.id, (view, events));
    }

    /// Moves to the chat before or after this one, wrapping around.
    pub fn step_chat(&mut self, step: isize, cx: &mut Context<Self>) {
        let open: Vec<SessionId> = self.open_sessions().map(|s| s.id.clone()).collect();
        let Some(current) = open.iter().position(|id| Some(id) == self.active.as_ref()) else { return };
        let next = crate::app::wrapped_step(current, step, open.len());
        self.activate(open[next].clone(), cx);
    }

    /// Asks first if it would stop an agent that's working.
    pub fn close_chat(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let working = self.sessions.iter().any(|s| s.id == id && is_working(s));
        if !working {
            self.finish_closing(id, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Close this chat?",
            Some("Its agent is still working, and closing the chat stops it. The history stays."),
            &["Close chat", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |t, cx| t.finish_closing(id, cx));
            }
        })
        .detach();
    }

    fn finish_closing(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let closed = cx.background_spawn(async move { client.session_close(&id) }).await;
            let _ = this.update(cx, |t, cx| match closed {
                Ok(session) => t.set_session(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't close the chat: {e}"))),
            });
        })
        .detach();
    }

    pub fn reopen_chat(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let reopened = cx.background_spawn(async move { client.session_reopen(&id) }).await;
            let _ = this.update(cx, |t, cx| match reopened {
                Ok(session) => {
                    let id = session.id.clone();
                    t.set_session(session, cx);
                    t.activate(id, cx);
                }
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't reopen the chat: {e}"))),
            });
        })
        .detach();
    }

    /// Changes the showing chat's model or approval mode. The hub only agrees once a running agent
    /// has, so the pickers change when this comes back, and a refusal leaves them as they were.
    pub fn configure(&mut self, model: Option<String>, mode: Option<String>, cx: &mut Context<Self>) {
        let Some(id) = self.active.clone() else { return };
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let changed = cx.background_spawn(async move { client.session_configure(&id, model, mode) }).await;
            let _ = this.update(cx, |t, cx| match changed {
                Ok(session) => t.set_session(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't change that: {e}"))),
            });
        })
        .detach();
    }

    pub fn set_workspace(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        self.workspace = workspace;
        // Picks up the last lines of the log once setup has finished.
        if self.setup_log.is_some() {
            self.load_setup_log(cx);
        }
        cx.notify();
    }

    /// Shows or hides the setup log. It stays current until setup finishes.
    pub fn toggle_setup_log(&mut self, cx: &mut Context<Self>) {
        if self.setup_log.take().is_some() {
            self.setup_log_refresh = None;
            cx.notify();
            return;
        }
        self.setup_log = Some(String::new());
        self.load_setup_log(cx);
        self.setup_log_refresh = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SETUP_LOG_REFRESH).await;
                let running = this.update(cx, |t, cx| {
                    let running = t.workspace.setup == SetupStatus::Running;
                    if running {
                        t.load_setup_log(cx);
                    }
                    running
                });
                if !matches!(running, Ok(true)) {
                    return;
                }
            }
        }));
        cx.notify();
    }

    fn load_setup_log(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        cx.spawn(async move |this, cx| {
            let log = cx.background_spawn(async move { client.workspace_setup_log(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                if t.setup_log.is_some() {
                    t.setup_log = Some(log.unwrap_or_else(|e| format!("Couldn't read the log: {e}")));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn rerun_setup(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        cx.spawn(async move |this, cx| {
            let started = cx.background_spawn(async move { client.workspace_rerun_setup(&id) }).await;
            let _ = this.update(cx, |t, cx| match started {
                Ok(workspace) => t.set_workspace(workspace, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't run setup: {e}"))),
            });
        })
        .detach();
    }

    /// A line above the composer while setup matters: when it's running, or if it failed.
    fn setup_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (message, running) = match self.workspace.setup {
            SetupStatus::Running => ("Setting up this workspace. Messages you send now wait until it's ready.", true),
            SetupStatus::Failed { code: Some(_) } => ("Setup failed, so the agent may find things missing.", false),
            SetupStatus::Failed { code: None } => ("Setup didn't finish, so the agent may find things missing.", false),
            _ => return None,
        };
        let theme = cx.theme();
        let (border, muted, warning) = (theme.border, theme.muted_foreground, theme.warning);
        let log_label = if self.setup_log.is_some() { "Hide log" } else { "View log" };
        Some(
            h_flex()
                .flex_none()
                .w_full()
                .px_3()
                .py_1()
                .gap_2()
                .border_t_1()
                .border_color(border)
                .text_xs()
                .text_color(if running { muted } else { warning })
                .child(div().flex_1().min_w_0().child(message))
                .child(
                    Button::new("setup-log")
                        .ghost()
                        .xsmall()
                        .label(log_label)
                        .on_click(cx.listener(|t, _, _, cx| t.toggle_setup_log(cx))),
                )
                .child(if running {
                    Button::new("stop-setup")
                        .ghost()
                        .xsmall()
                        .label("Stop")
                        .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                        .into_any_element()
                } else {
                    Button::new("rerun-setup")
                        .ghost()
                        .xsmall()
                        .label("Run again")
                        .on_click(cx.listener(|t, _, _, cx| t.rerun_setup(cx)))
                        .into_any_element()
                })
                .into_any_element(),
        )
    }

    pub fn set_session(&mut self, session: Session, cx: &mut Context<Self>) {
        match self.sessions.iter_mut().find(|s| s.id == session.id) {
            Some(existing) => *existing = session,
            None => self.sessions.push(session),
        }
        self.keep_active_valid(cx);
        cx.notify();
    }

    /// After reconnecting, or anything else that means the list has to be read again.
    pub fn set_sessions(&mut self, sessions: Vec<Session>, cx: &mut Context<Self>) {
        self.sessions = sessions;
        self.keep_active_valid(cx);
        cx.notify();
    }

    /// Moves off a chat that's been closed, here or anywhere else, and starts one if none are left.
    fn keep_active_valid(&mut self, cx: &mut Context<Self>) {
        let closed: Vec<SessionId> =
            self.sessions.iter().filter(|s| s.closed_at_ms.is_some()).map(|s| s.id.clone()).collect();
        for id in &closed {
            self.transcripts.remove(id);
            self.drafts.remove(id);
        }
        let open: Vec<SessionId> = self.open_sessions().map(|s| s.id.clone()).collect();
        if self.active.as_ref().is_some_and(|id| open.contains(id)) {
            return;
        }
        // Where the closed one was, counting only the chats before it that are still open.
        let position = self
            .active
            .as_ref()
            .and_then(|active| self.sessions.iter().position(|s| &s.id == active))
            .map_or(0, |at| self.sessions[..at].iter().filter(|s| s.closed_at_ms.is_none()).count());
        self.active = None;
        if let Some(id) = neighbor(&open, position) {
            self.activate(id, cx);
        } else {
            self.show_draft(cx);
            self.ensure_chat(cx);
        }
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let Some(transcript) = self.transcript() else { return };
        // While the agent is waiting on a question, what's typed is the answer to it.
        let taken = transcript.update(cx, |t, cx| t.answer_with_text(&text, cx) || t.send_prompt(text, cx));
        if taken {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        }
    }

    pub fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, Transcript::cancel);
        }
    }

    pub fn create_pull_request(&mut self, cx: &mut Context<Self>) {
        let prompt = pull_request_prompt(&self.workspace);
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, |t, cx| t.send_prompt(prompt, cx));
        }
    }

    pub fn refresh_pull_request(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { client.workspace_pull_request(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                // A failure here only means no badge, and it's not worth interrupting anyone over.
                t.pull_request = found.unwrap_or(None);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toggle_diff(&mut self, cx: &mut Context<Self>) {
        self.diff_open = !self.diff_open;
        if self.diff_open {
            match &self.diff {
                Some(diff) => diff.update(cx, DiffPanel::refresh),
                None => self.make_diff_panel(cx),
            }
        }
        cx.notify();
    }

    /// This needs the window, and a click handler is already inside a window update, so it runs just
    /// after whatever called it.
    fn make_diff_panel(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| {
                    let (client, id) = (t.client.clone(), t.workspace.id.clone());
                    let diff = cx.new(|cx| DiffPanel::new(client, id, window, cx));
                    let events = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| match event {
                        DiffEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
                        DiffEvent::SendToAgent(prompt) => {
                            if let Some(transcript) = this.transcript() {
                                transcript.update(cx, |t, cx| t.send_prompt(prompt.clone(), cx));
                            }
                        }
                    });
                    t.subscriptions.push(events);
                    t.diff = Some(diff);
                    cx.notify();
                });
            });
        });
    }

    /// One tab for each open chat, then a way to start another, and any closed ones to go back to.
    fn chat_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let numbered: Vec<(usize, Session)> = self.sessions.iter().cloned().enumerate().collect();
        let tabs: Vec<AnyElement> = numbered
            .iter()
            .filter(|(_, s)| s.closed_at_ms.is_none())
            .map(|(index, session)| {
                let selected = self.active.as_ref() == Some(&session.id);
                let dot = crate::sidebar::status_color(Some(session.status), cx);
                let (to_show, to_close) = (session.id.clone(), session.id.clone());
                h_flex()
                    .flex_none()
                    .max_w(px(220.))
                    .child(
                        Button::new(ElementId::Name(format!("chat-{}", session.id).into()))
                            .ghost()
                            .xsmall()
                            .flex_1()
                            .min_w_0()
                            .selected(selected)
                            .on_click(cx.listener(move |t, _, _, cx| t.activate(to_show.clone(), cx)))
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .when(!selected, |d| d.text_color(muted))
                                    .child(div().flex_none().size_2().rounded_full().bg(dot))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(chat_title(session, index + 1)),
                                    ),
                            ),
                    )
                    .child(
                        Button::new(ElementId::Name(format!("close-chat-{}", session.id).into()))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip("Close chat")
                            .on_click(cx.listener(move |t, _, window, cx| t.close_chat(to_close.clone(), window, cx))),
                    )
                    .into_any_element()
            })
            .collect();
        let closed: Vec<(SessionId, String)> = numbered
            .iter()
            .filter(|(_, s)| s.closed_at_ms.is_some())
            .map(|(index, s)| (s.id.clone(), chat_title(s, index + 1)))
            .collect();
        let this = cx.weak_entity();
        h_flex()
            .id("chat-tabs")
            .flex_none()
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(border)
            .overflow_x_scroll()
            .children(tabs)
            .child(
                Button::new("new-chat")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip("New chat")
                    .on_click(cx.listener(|t, _, _, cx| t.new_chat(cx))),
            )
            .when(!closed.is_empty(), |d| {
                d.child(
                    Button::new("closed-chats")
                        .ghost()
                        .xsmall()
                        .label(format!("Closed ({})", closed.len()))
                        .dropdown_menu(move |mut menu, _, _| {
                            for (id, title) in &closed {
                                let (this, id) = (this.clone(), id.clone());
                                menu = menu.item(PopupMenuItem::new(title.clone()).on_click(move |_, _, cx| {
                                    let _ = this.update(cx, |t, cx| t.reopen_chat(id.clone(), cx));
                                }));
                            }
                            menu
                        }),
                )
            })
    }

    /// The model and approval mode of the chat showing.
    fn pickers(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let model = session.and_then(|s| s.model.clone());
        let mode = session.and_then(|s| s.permission_mode.clone());
        let full_access = mode.as_deref() == Some("bypassPermissions");
        let warning = cx.theme().warning;
        let mode_text = mode.as_deref().map_or("Your settings", mode_label).to_string();
        let this = cx.weak_entity();
        let models = {
            let (this, current) = (this.clone(), model.clone().unwrap_or_else(|| "default".to_string()));
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for (id, label) in MODELS {
                    let this = this.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(current == id).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |t, cx| t.configure(Some(id.to_string()), None, cx));
                    }));
                }
                menu
            }
        };
        let approval_modes = {
            let current = mode.clone();
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for id in MODES {
                    let this = this.clone();
                    let checked = current.as_deref() == Some(id);
                    menu = menu.item(PopupMenuItem::new(mode_label(id).to_string()).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = this.update(cx, |t, cx| t.configure(None, Some(id.to_string()), cx));
                        },
                    ));
                }
                menu
            }
        };
        h_flex()
            .flex_none()
            .w_full()
            .px_3()
            .pb_2()
            .gap_1()
            .child(
                Button::new("model-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .label(model_label(model.as_deref()))
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, models),
            )
            .child(
                Button::new("mode-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .when(full_access, |b| b.text_color(warning))
                    .label(mode_text)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, approval_modes),
            )
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let id = self.workspace.id.clone();
        let pr = self.pull_request.clone();
        h_flex()
            .flex_none()
            .w_full()
            .pl_3()
            .pr_3()
            .py_2()
            .gap_3()
            .border_b_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(self.workspace.name.clone()))
                    .child(div().text_xs().text_color(muted).overflow_hidden().whitespace_nowrap().child(format!(
                        "{} from {}",
                        self.workspace.branch,
                        self.workspace.base_ref.as_deref().unwrap_or("?")
                    ))),
            )
            .when_some(pr, |d, pr| {
                let url = pr.url.clone();
                d.child(
                    Button::new("pull-request")
                        .label(format!("#{} {}", pr.number, pr.state.to_lowercase()))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .when(self.pull_request.is_none(), |d| {
                d.child(
                    Button::new("create-pr")
                        .label("Create PR")
                        .on_click(cx.listener(|t, _, _, cx| t.create_pull_request(cx))),
                )
            })
            .child(
                Button::new("toggle-diff")
                    .label(if self.diff_open { "Hide diff" } else { "Diff" })
                    .on_click(cx.listener(|t, _, _, cx| t.toggle_diff(cx))),
            )
            .child(Button::new("archive").label("Archive").on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(PaneEvent::Archive(id.clone(), false));
            })))
    }
}

impl Render for WorkspacePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (border, muted) = (cx.theme().border, cx.theme().muted_foreground);
        let working = self.session().is_some_and(is_working);
        let interrupted = self.session().is_some_and(|s| s.status == SessionStatus::Interrupted);
        let setup_line = self.setup_line(cx);
        let header = self.header(cx).into_any_element();
        let tabs = self.chat_tabs(cx).into_any_element();
        let pickers = self.pickers(cx).into_any_element();
        let log = self.setup_log.clone();
        let transcript = self.transcript();
        let diff = if self.diff_open { self.diff.clone() } else { None };
        v_flex().size_full().child(header).child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(tabs)
                        .child(div().flex_1().min_h_0().w_full().when_some(transcript, ParentElement::child))
                        .when_some(log, |d, text| {
                            d.child(
                                div()
                                    .id("setup-log-text")
                                    .flex_none()
                                    .w_full()
                                    .max_h(px(220.))
                                    .overflow_y_scroll()
                                    .px_3()
                                    .py_2()
                                    .border_t_1()
                                    .border_color(border)
                                    .font_family("Menlo")
                                    .text_size(px(12.))
                                    .text_color(muted)
                                    .child(SharedString::from(if text.is_empty() {
                                        "Nothing yet.".to_string()
                                    } else {
                                        text
                                    })),
                            )
                        })
                        .children(setup_line)
                        .when(interrupted, |d| {
                            d.child(div().flex_none().w_full().px_3().py_1().text_xs().text_color(muted).child(
                                "The agent stopped before it finished. Send a message to pick up where it left off.",
                            ))
                        })
                        .child(
                            h_flex()
                                .flex_none()
                                .w_full()
                                .items_end()
                                .px_3()
                                .pt_3()
                                .pb_1()
                                .gap_2()
                                .border_t_1()
                                .border_color(border)
                                .child(div().flex_1().min_w_0().child(Textarea::new(&self.composer)))
                                .child(if working {
                                    Button::new("stop")
                                        .label("Stop")
                                        .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                                        .into_any_element()
                                } else {
                                    Button::new("send")
                                        .primary()
                                        .label("Send")
                                        .on_click(cx.listener(|t, _, window, cx| t.submit(window, cx)))
                                        .into_any_element()
                                }),
                        )
                        .child(pickers),
                )
                .when_some(diff, |d, diff| {
                    d.child(div().flex_1().min_w_0().h_full().border_l_1().border_color(border).child(diff))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{MODELS, inherited_mode, model_label, neighbor};

    #[test]
    fn a_new_chat_doesnt_inherit_plan_or_full_access() {
        assert_eq!(inherited_mode(Some("plan")), None);
        assert_eq!(inherited_mode(Some("bypassPermissions")), None);
        assert_eq!(inherited_mode(Some("acceptEdits")).as_deref(), Some("acceptEdits"));
        assert_eq!(inherited_mode(None), None);
    }

    #[test]
    fn closing_a_chat_moves_to_the_next_or_else_the_last() {
        let open: Vec<String> = vec!["a".into(), "c".into()];
        assert_eq!(neighbor(&open, 1).as_deref(), Some("c"));
        assert_eq!(neighbor(&open, 2).as_deref(), Some("c"));
        assert_eq!(neighbor(&[], 0), None);
    }

    #[test]
    fn a_model_with_no_name_here_still_shows() {
        assert_eq!(model_label(None), MODELS[0].1);
        assert_eq!(model_label(Some("opus")), "Opus");
        assert_eq!(model_label(Some("claude-something-new")), "claude-something-new");
    }
}
