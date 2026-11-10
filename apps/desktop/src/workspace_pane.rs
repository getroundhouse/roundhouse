//! One workspace: its chats, the composer, the diff tab, and the sidebar of changes beside them when it's open.

use std::collections::HashMap;

use client_core::{Client, RecordsFeed};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, Entity, EventEmitter, Focusable as _, Pixels, SharedString,
    Subscription, Task, Window, px,
};
use protocol::model::{DiffComment, Effort, Session, SessionId, SessionOptions, Workspace, WorkspaceId};
use transcript::Transcript;

use crate::attached::Attachments;
use crate::busy::Busy;
use crate::changes::Changes;
use crate::file_diff::FileDiff;
use crate::pull_request::Lookup;
use crate::review::Review;
use attaching::Draft;
pub use reading::Mark;
use reading::Reading;

mod attaching;
mod chats;
mod closed;
mod composer;
mod context;
mod diff_tab;
mod pull_request;
mod queue;
mod reading;
mod setup;
#[cfg(test)]
mod tests;
mod view;

/// What shows under the tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Showing {
    /// The active chat.
    #[default]
    Chat,
    /// The diff tab, in place of the active chat.
    Diff,
}

/// The workspace's diff and its two views, made together the first time the sidebar of changes opens, then kept.
pub struct Diff {
    pub review: Entity<Review>,
    /// What the diff tab shows.
    pub file: Entity<FileDiff>,
    /// The sidebar of changes.
    pub changes: Entity<Changes>,
    /// The diff tab, while it's open. It's here so there's never a diff tab without a diff to show in it.
    tab: Option<DiffTab>,
}

struct DiffTab {
    path: SharedString,
    /// It covers the active chat.
    in_front: bool,
}

#[derive(Clone, Debug)]
pub enum PaneEvent {
    Error(String),
    Archive(WorkspaceId, bool),
}

pub struct WorkspacePane {
    client: Client,
    /// Client-core's copy of the hub's records, which replies go into.
    records: RecordsFeed,
    pub workspace: Workspace,
    /// In the order they were made, including closed ones.
    pub sessions: Vec<Session>,
    /// A chat to show once client-core's copy has it, such as one just made.
    showing_next: Option<SessionId>,
    /// What client-core holds for this workspace, for the diff.
    comments: Vec<DiffComment>,
    pub active: Option<SessionId>,
    /// Made the first time a chat is shown, then kept so it stays current and keeps its place.
    transcripts: HashMap<SessionId, (Entity<Transcript>, Subscription)>,
    /// What's been typed and attached in the chats not showing.
    drafts: HashMap<SessionId, Draft>,
    /// What's attached in the chat showing, to go with what's typed.
    attached: Attachments,
    /// A prompt handed back before the workspace had a chat to show, for the first that shows.
    waiting: Option<Draft>,
    /// A new chat is on its way, so another press doesn't make a second.
    creating_chat: bool,
    /// A queued prompt being changed.
    queue_edit: Option<queue::Editing>,
    closed_menu: closed::Menu,
    /// Whose account the workspace's agents run under, for the card the context's ring shows.
    account: context::AccountRead,
    busy: Busy,
    reading: Reading,
    pub composer: Entity<TextareaState>,
    pub diff: Option<Diff>,
    pub pull_request: Lookup,
    /// How many times the pull request has been asked for, so only the newest answer is shown.
    pull_request_asks: u64,
    pub setup_log: Option<String>,
    /// Keeps the open log current while setup runs.
    setup_log_refresh: Option<Task<()>>,
    /// Set on a new connection, until [`Self::catch_up`] reads again what it may have changed.
    behind: bool,
    window: AnyWindowHandle,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaneEvent> for WorkspacePane {}

/// The workspace header's height, which is also the strip along the window's top that moves it. The window's buttons
/// sit in the sidebar's part of the strip, and the sidebar's list starts level with its bottom edge, so the two line up.
pub(crate) const HEADER_HEIGHT: Pixels = px(40.);

impl WorkspacePane {
    /// What's typed here and not sent, which goes when the app quits: how many chats have something in their
    /// composer, and how many comments on the diff are being written.
    pub(crate) fn unsent(&self, cx: &App) -> (usize, usize) {
        let typed = |text: &str| !text.trim().is_empty();
        let showing = usize::from(typed(&self.composer.read(cx).text().to_string()) || !self.attached.is_empty());
        let waiting = self.waiting.iter();
        let others = self.drafts.values().chain(waiting).filter(|draft| draft.has_anything()).count();
        let draft = self.diff.as_ref().and_then(|diff| diff.file.read(cx).draft.as_ref());
        let comment = draft.is_some_and(|draft| typed(&draft.input.read(cx).text().to_string()));
        (showing + others, usize::from(comment))
    }

    /// Whether the app is making, archiving or restoring a workspace, which the pull request's Archive waits for.
    pub(crate) fn set_busy(&mut self, busy: Busy, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    /// Where the workspace's work stands, as its mark and the pull request's bar show it.
    pub(crate) fn work(&self) -> look::Work {
        self.pull_request.work()
    }

    pub fn new(
        client: Client,
        records: RecordsFeed,
        workspace: Workspace,
        sessions: Vec<Session>,
        comments: Vec<DiffComment>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                // Room for a paragraph before it grows, so a longer prompt doesn't start out cramped.
                .auto_grow(4, 12)
                .submit_on_enter(true)
                .placeholder("Ask the agent to do something")
        });
        // While the agent works, Enter steers it, and Cmd-Enter queues what's typed for after it finishes.
        let on_enter = cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, secondary } = event {
                if *secondary && this.session().is_some_and(|s| s.status.is_active()) {
                    this.queue_typed(window, cx);
                } else {
                    this.submit(window, cx);
                }
            }
        });
        let on_settings = crate::settings::observe(cx, |this, cx| {
            for (transcript, _) in this.transcripts.values() {
                transcript.update(cx, |t, cx| t.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx));
            }
        });
        let pull_request = Lookup::recorded(&workspace).unwrap_or(Lookup::Looking);
        let mut this = Self {
            client,
            records,
            workspace,
            sessions,
            showing_next: None,
            comments,
            active: None,
            transcripts: HashMap::new(),
            drafts: HashMap::new(),
            attached: Attachments::default(),
            waiting: None,
            creating_chat: false,
            queue_edit: None,
            closed_menu: closed::Menu::Shut,
            account: context::AccountRead::default(),
            busy: Busy::Free,
            reading: Reading::default(),
            composer,
            diff: None,
            pull_request,
            pull_request_asks: 0,
            setup_log: None,
            setup_log_refresh: None,
            behind: false,
            window: window.window_handle(),
            subscriptions: vec![on_enter, on_settings],
        };
        // The chat last worked in.
        let latest = this.open_sessions().max_by_key(|s| (s.updated_at_ms, s.created_at_ms)).map(|s| s.id.clone());
        match latest {
            Some(id) => this.activate(id, cx),
            None => this.ensure_chat(cx),
        }
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

    /// Changes the showing chat's model, approval mode or effort. The hub only agrees once a running agent
    /// has, so the pickers change when this comes back, and a refusal leaves them as they were.
    pub fn configure(
        &mut self,
        model: Option<String>,
        mode: Option<String>,
        effort: Option<Effort>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.active.clone() else { return };
        let options = SessionOptions { model, permission_mode: mode, effort };
        let changing = self.client.start(move |client| client.session_configure(&id, options));
        cx.spawn(async move |this, cx| {
            let changed = changing.recv().await;
            let _ = this.update(cx, |t, cx| match changed {
                Ok(session) => t.took_reply(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't change that: {e}"))),
            });
        })
        .detach();
    }

    /// Carries on on a new connection to the hub, keeping everything shown and typed. What the hub reads from
    /// the worktree waits for [`Self::catch_up`], so a reconnect doesn't start a worker for every pane at once.
    pub fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        self.forget_account();
        for (transcript, _) in self.transcripts.values() {
            transcript.update(cx, |t, _| t.reconnect(client));
        }
        if let Some(diff) = &self.diff {
            diff.review.update(cx, |r, _| r.reconnect(client));
        }
        self.behind = true;
    }

    /// For when the pane is shown after a reconnect, to read again what it may have changed.
    pub fn catch_up(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.behind) {
            self.shown(cx);
        }
    }

    /// For each time the pane is shown. Reads the pull request again, since it changes on GitHub while nobody's
    /// looking, and the diff, if the sidebar of changes or the diff tab shows it.
    pub fn shown(&mut self, cx: &mut Context<Self>) {
        self.behind = false;
        self.refresh_pull_request(cx);
        self.load_changes(cx);
    }

    /// What client-core holds for this workspace now. Only what changed is passed on.
    pub fn set_records(
        &mut self,
        workspace: Workspace,
        sessions: Vec<Session>,
        comments: Vec<DiffComment>,
        cx: &mut Context<Self>,
    ) {
        if workspace != self.workspace {
            if let Some(recorded) = Lookup::recorded(&workspace) {
                self.pull_request = recorded;
            }
            self.workspace = workspace;
            // Picks up the last lines of the log once setup has finished.
            if self.setup_log.is_some() {
                self.load_setup_log(cx);
            }
        }
        if sessions != self.sessions {
            self.set_sessions(sessions, cx);
        }
        if comments != self.comments {
            if let Some(diff) = &self.diff {
                diff.review.update(cx, |r, cx| r.set_comments(comments.clone(), cx));
            }
            self.comments = comments;
        }
        cx.notify();
    }

    /// Runs `then` with the window, which a click handler is already inside an update of, just after whatever called
    /// this.
    fn with_window(
        &self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| then(t, window, cx));
            });
        });
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    /// What's in front under the tabs.
    pub fn showing(&self) -> Showing {
        let tab = self.diff.as_ref().and_then(|diff| diff.tab.as_ref());
        if tab.is_some_and(|tab| tab.in_front) { Showing::Diff } else { Showing::Chat }
    }

    /// Puts the active chat or the diff tab in front, as long as the diff tab is open. It's the only way either comes
    /// to the front, so what's read and what has focus always follow what's drawn.
    pub(super) fn bring_to_front(&mut self, front: Showing, cx: &mut Context<Self>) {
        if let Some(tab) = self.diff.as_mut().and_then(|diff| diff.tab.as_mut()) {
            tab.in_front = front == Showing::Diff;
        }
        // The chat is read only while it's in front.
        self.read_showing(cx);
        // Not behind a dialog, where typing meant for it would come here unseen. The app focuses the pane showing once
        // the dialog closes.
        self.with_window(cx, |t, window, cx| {
            if !window.has_active_dialog(cx) {
                t.focus_showing(window, cx);
            }
        });
        cx.notify();
    }

    /// Focuses the diff while the diff tab shows, and the composer otherwise. The composer isn't drawn behind the diff
    /// tab, and focus on something that isn't drawn takes the window's keys, such as `Ctrl-Tab`, nowhere.
    pub fn focus_showing(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.showing() {
            Showing::Diff => {
                if let Some(diff) = &self.diff {
                    window.focus(&diff.file.focus_handle(cx), cx);
                }
            }
            Showing::Chat => self.focus_composer(window, cx),
        }
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let Some(transcript) = self.transcript() else { return };
        // While the agent is waiting on a question, what's typed is the answer to it, and the files wait for the next
        // prompt, since an answer can't carry them.
        if transcript.update(cx, |t, cx| t.answer_with_text(&text, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
            return;
        }
        let files = self.attached.take();
        if transcript.update(cx, |t, cx| t.send_prompt_with(text, files, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        }
        cx.notify();
    }

    pub fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, Transcript::cancel);
        }
    }
}
