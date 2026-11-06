//! One workspace: its transcript and composer, and the diff beside them when it's open.

use client_core::Client;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use protocol::model::{AgentKind, PullRequest, Session, SessionStatus, Workspace, WorkspaceId};
use transcript::{Transcript, TranscriptEvent};

use crate::diff_panel::{DiffEvent, DiffPanel};

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
    pub session: Option<Session>,
    pub transcript: Option<Entity<Transcript>>,
    pub composer: Entity<TextareaState>,
    pub diff: Option<Entity<DiffPanel>>,
    pub diff_open: bool,
    pub pull_request: Option<PullRequest>,
    window: AnyWindowHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaneEvent> for WorkspacePane {}

impl WorkspacePane {
    pub fn new(
        client: Client,
        workspace: Workspace,
        session: Option<Session>,
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
            session: None,
            transcript: None,
            composer,
            diff: None,
            diff_open: false,
            pull_request: None,
            window: window.window_handle(),
            _subscriptions: vec![on_enter],
        };
        if let Some(session) = session {
            this.adopt_session(session, cx);
        } else {
            this.create_session(cx);
        }
        this.refresh_pull_request(cx);
        this
    }

    /// A workspace restored or made elsewhere may have no session yet.
    fn create_session(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace.id.clone());
        cx.spawn(async move |this, cx| {
            let created =
                cx.background_spawn(async move { client.session_create(&id, AgentKind::Claude, None, None) }).await;
            let _ = this.update(cx, |t, cx| match created {
                Ok(session) => t.adopt_session(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't start a session: {e}"))),
            });
        })
        .detach();
    }

    fn adopt_session(&mut self, session: Session, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let view = cx.new(|cx| Transcript::new(client, &session, cx));
        let events = cx.subscribe(&view, |this, _, event: &TranscriptEvent, cx| match event {
            TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            TranscriptEvent::Disconnected => cx.emit(PaneEvent::Disconnected),
            TranscriptEvent::TurnEnded => {
                this.refresh_pull_request(cx);
                if let Some(diff) = &this.diff {
                    diff.update(cx, |d, cx| d.refresh(cx));
                }
            }
        });
        self._subscriptions.push(events);
        self.transcript = Some(view);
        self.session = Some(session);
        cx.notify();
    }

    pub fn set_workspace(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        self.workspace = workspace;
        cx.notify();
    }

    pub fn set_session(&mut self, session: Session, cx: &mut Context<Self>) {
        if self.session.as_ref().is_some_and(|s| s.id == session.id) {
            self.session = Some(session);
            cx.notify();
        } else if self.session.is_none() {
            self.adopt_session(session, cx);
        }
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    fn is_working(&self) -> bool {
        matches!(self.session.as_ref().map(|s| s.status), Some(SessionStatus::Running | SessionStatus::WaitingApproval))
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let Some(transcript) = self.transcript.clone() else { return };
        if transcript.update(cx, |t, cx| t.send_prompt(text, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        }
    }

    pub fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = &self.transcript {
            transcript.update(cx, |t, cx| t.cancel(cx));
        }
    }

    pub fn create_pull_request(&mut self, cx: &mut Context<Self>) {
        let prompt = pull_request_prompt(&self.workspace);
        if let Some(transcript) = &self.transcript {
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
                Some(diff) => diff.update(cx, |d, cx| d.refresh(cx)),
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
                            if let Some(transcript) = &this.transcript {
                                transcript.update(cx, |t, cx| t.send_prompt(prompt.clone(), cx));
                            }
                        }
                    });
                    t._subscriptions.push(events);
                    t.diff = Some(diff);
                    cx.notify();
                });
            });
        });
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
        let working = self.is_working();
        let interrupted = self.session.as_ref().is_some_and(|s| s.status == SessionStatus::Interrupted);
        let header = self.header(cx);
        let transcript = self.transcript.clone();
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
                        .child(div().flex_1().min_h_0().w_full().when_some(transcript, |d, t| d.child(t)))
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
                                .p_3()
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
                        ),
                )
                .when_some(diff, |d, diff| {
                    d.child(div().flex_1().min_w_0().h_full().border_l_1().border_color(border).child(diff))
                }),
        )
    }
}
