//! One workspace: its chats, the composer, and the diff beside them when it's open.

use std::collections::HashMap;

use client_core::Client;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AnyWindowHandle, AppContext, Context, Entity, EventEmitter, Subscription, Task, Window};
use protocol::model::{PullRequest, Session, SessionId, SessionStatus, Workspace, WorkspaceId};
use transcript::Transcript;

use crate::diff_panel::{DiffEvent, DiffPanel};

mod chats;
mod setup;
#[cfg(test)]
mod tests;
mod view;

fn is_working(session: &Session) -> bool {
    matches!(session.status, SessionStatus::Running | SessionStatus::WaitingApproval)
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
}
