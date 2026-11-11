//! One workspace: its chats, the composer, the file tab, and the sidebar of changes beside them when it's open.

use std::collections::HashMap;

use client_core::{Client, RecordsFeed};
use gpui_kit::component::input::TextareaState;
use gpui_kit::{AnyWindowHandle, App, Context, Entity, EventEmitter, Pixels, Subscription, Task, Window, px};
use protocol::model::{Account, AccountId, DiffComment, Session, SessionId, Workspace, WorkspaceId};
use transcript::Transcript;

use crate::all_files::AllFiles;
use crate::attached::Attachments;
use crate::busy::Busy;
use crate::changes::Changes;
use crate::file_diff::FileDiff;
use crate::file_view::FileView;
use crate::pull_request::Lookup;
use crate::review::Review;
use crate::sidebar_tabs::TabShown;
use attaching::{Draft, Waiting};
pub(crate) use context::accounts_named;
use file_tab::FileTab;
pub use file_tab::Opened;
pub use reading::Mark;
use reading::Reading;

mod attaching;
mod changes_sidebar;
mod chats;
mod closed;
mod composer;
mod context;
mod file_tab;
mod focus;
mod front;
mod pull_request;
mod queue;
mod reading;
mod setup;
mod subagent_tab;
#[cfg(test)]
mod tests;
mod view;
mod waiting;

/// What shows under the tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Showing {
    /// The active chat.
    #[default]
    Chat,
    /// The file tab, in place of the active chat.
    File,
    /// The subagent tab, in place of the active chat.
    Subagent,
}

/// The workspace's diff and the views of its worktree, made together the first time the sidebar of changes opens, then
/// kept.
pub struct Diff {
    pub review: Entity<Review>,
    /// What the file tab shows of a file opened from Changes.
    pub file: Entity<FileDiff>,
    /// What the file tab shows of a file opened from All files.
    pub viewer: Entity<FileView>,
    /// The sidebar of changes' list of them.
    pub changes: Entity<Changes>,
    /// Every file in the worktree, which the sidebar of changes shows in place of the changes when it's chosen.
    pub all_files: Entity<AllFiles>,
    /// Which of the two the sidebar of changes shows.
    sidebar_tab: TabShown,
    /// The file tab, while it's open. It's here so there's never a file tab without a view to show in it.
    tab: Option<FileTab>,
    /// A turn ended while the pane wasn't on screen, so the files are listed again once it is.
    files_behind: bool,
}

#[derive(Clone, Debug)]
pub enum PaneEvent {
    Error(String),
    Archive(WorkspaceId, bool),
    /// Title the workspace again from what's been asked in it.
    RegenerateTitle(WorkspaceId),
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
    waiting: Option<Waiting>,
    /// A new chat is on its way, so another press doesn't make a second.
    creating_chat: bool,
    /// A queued prompt being changed.
    queue_edit: Option<queue::Editing>,
    /// Prompts queued from here that the hub hasn't answered for yet, oldest first.
    queue_pending: Vec<queue::Pending>,
    closed_menu: closed::Menu,
    /// The records of the accounts the workspace's chats name, which say who each is and what's left of its limits,
    /// for what sits beside Send.
    accounts: HashMap<AccountId, Account>,
    busy: Busy,
    /// The app is waiting on the hub for a new title for this workspace.
    retitling: bool,
    /// Whether Shift-Tab last asked each chat whose hub hasn't answered yet to plan.
    plan_asked: HashMap<SessionId, bool>,
    reading: Reading,
    pub composer: Entity<TextareaState>,
    pub diff: Option<Diff>,
    subagents: subagent_tab::Subagents,
    pub pull_request: Lookup,
    /// How many times the pull request has been asked for, so only the newest answer is shown.
    pull_request_asks: u64,
    pub setup_log: Option<String>,
    /// Keeps the open log current while setup runs.
    setup_log_refresh: Option<Task<()>>,
    /// Set on a new connection, until [`Self::catch_up`] reads again what it may have changed.
    behind: bool,
    /// Away while the app connects to a hub again, with this still on screen. `composer.rs` has what that holds up.
    hub: composer::Hub,
    window: AnyWindowHandle,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaneEvent> for WorkspacePane {}

/// The workspace header's height, which is also the strip along the window's top that moves it. The window's buttons
/// sit in the sidebar's part of the strip, and the sidebar's list starts level with its bottom edge, so the two line up.
pub(crate) const HEADER_HEIGHT: Pixels = px(40.);

impl WorkspacePane {
    /// What's typed here and not sent, which goes when the app quits: how many chats have something in their
    /// composer or typed as an answer to a question, and how many comments on the diff are being written.
    pub(crate) fn unsent(&self, cx: &App) -> (usize, usize) {
        let typed = |text: &str| !text.trim().is_empty();
        let answering = |id: &SessionId| {
            let question = self.transcripts.get(id).and_then(|(transcript, _)| transcript.read(cx).question());
            question.is_some_and(|question| question.read(cx).has_typed(cx))
        };
        let in_composer = typed(&self.composer.read(cx).text().to_string()) || !self.attached.is_empty();
        let showing = usize::from(in_composer || self.active.as_ref().is_some_and(answering));
        let drafted = |id: &SessionId| self.drafts.get(id).is_some_and(Draft::has_anything);
        let ids = self.drafts.keys().chain(self.transcripts.keys().filter(|id| !self.drafts.contains_key(*id)));
        let others = ids.filter(|id| self.active.as_ref() != Some(*id) && (drafted(id) || answering(id))).count();
        let waiting = usize::from(self.waiting.as_ref().is_some_and(|waiting| waiting.draft.has_anything()));
        let draft = self.diff.as_ref().and_then(|diff| diff.file.read(cx).draft.as_ref());
        let comment = draft.is_some_and(|draft| typed(&draft.input.read(cx).text().to_string()));
        (showing + others + waiting, usize::from(comment))
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
        let (composer, on_enter) = composer::new_input(window, cx);
        let on_settings = crate::settings::observe(cx, |this, cx| {
            for transcript in this.every_transcript() {
                transcript.update(cx, |t, cx| t.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx));
            }
        });
        let pull_request = Lookup::recorded(&workspace).unwrap_or(Lookup::Looking);
        let accounts = records.read(|held| accounts_named(held, &sessions));
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
            queue_pending: Vec::new(),
            closed_menu: closed::Menu::Shut,
            accounts,
            busy: Busy::Free,
            retitling: false,
            plan_asked: HashMap::new(),
            reading: Reading::default(),
            composer,
            diff: None,
            subagents: subagent_tab::Subagents::default(),
            pull_request,
            pull_request_asks: 0,
            setup_log: None,
            setup_log_refresh: None,
            behind: false,
            hub: composer::Hub::Here,
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

    /// Carries on on a new connection to the hub, keeping everything shown and typed. What the hub reads from
    /// the worktree waits for [`Self::catch_up`], so a reconnect doesn't start a worker for every pane at once.
    pub fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.client = client.clone();
        self.hub = composer::Hub::Here;
        for transcript in self.every_transcript() {
            transcript.update(cx, |t, _| t.reconnect(client));
        }
        if let Some(diff) = &mut self.diff {
            diff.sidebar_tab.reconnected();
            diff.review.update(cx, |r, _| r.reconnect(client));
            diff.all_files.update(cx, |a, _| a.reconnect(client));
            diff.viewer.update(cx, |v, _| v.reconnect(client));
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
    /// looking, and what the sidebar of changes and the file tab show of the worktree.
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
        accounts: HashMap<AccountId, Account>,
        cx: &mut Context<Self>,
    ) {
        self.accounts = accounts;
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
        self.pass_on_subagents(cx);
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

    pub fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, Transcript::cancel);
        }
    }
}
