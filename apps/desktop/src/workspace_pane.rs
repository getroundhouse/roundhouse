//! One workspace: its chats, the composer, the file tab, and the sidebar of changes beside them when it's open.

use std::collections::HashMap;

use client_core::records::InWorkspace;
use client_core::{Client, Drafts, RecordsFeed};
use gpui_kit::component::input::TextareaState;
use gpui_kit::{AnyWindowHandle, Context, Entity, EventEmitter, Pixels, Subscription, Task, Window, px};
use protocol::attachment::Attachment;
use protocol::model::{Seq, Session, SessionId, Workspace, WorkspaceId, WorkspaceStatus};
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
use attaching::{Composed, Waiting};
use file_tab::FileTab;
pub use file_tab::Opened;
pub(crate) use keeping::drafted;
pub use reading::Mark;
use reading::Reading;

mod agent;
mod attaching;
mod changes_sidebar;
mod chats;
mod closed;
mod composer;
mod context;
mod file_tab;
mod focus;
mod front;
mod keeping;
mod pull_request;
mod queue;
mod reading;
mod setup;
mod subagent_tab;
#[cfg(test)]
mod tests;
mod view;
mod waiting;
mod workspace_card;

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

/// What the pane holds for one chat, from the first time it holds anything for it until the chat closes, when it all
/// goes at once.
#[derive(Default)]
struct Chat {
    /// Made the first time the chat is shown, then kept so it stays current and keeps its place.
    transcript: Option<(Entity<Transcript>, Subscription)>,
    /// What's been typed and attached in it while it isn't showing.
    draft: Option<Composed>,
    /// The files the hub kept with its draft that haven't been read back yet.
    restoring: Option<Vec<Attachment>>,
    /// What Shift-Tab last asked, whether to plan, while the hub hasn't answered yet.
    plan_asked: Option<bool>,
    /// Its newest reply, while it's being marked read. It shows as read meanwhile, so a reply that comes while it's
    /// showing doesn't flash as unread first.
    marking: Option<Seq>,
}

#[derive(Clone, Debug)]
pub enum PaneEvent {
    Error(String),
    Archive(WorkspaceId, bool),
    /// Title the workspace again from what's been asked in it.
    RegenerateTitle(WorkspaceId),
    /// What's in front under the tabs changed, so the chat being read may have.
    Showing,
}

pub struct WorkspacePane {
    client: Client,
    /// Client-core's copy of the hub's records, which replies go into.
    records: RecordsFeed,
    /// What's shown of the workspace's records, as of client-core's last change: its own, its sessions' and its
    /// comments', and through them the accounts its sessions name and their subagents.
    shown: InWorkspace,
    /// A chat to show once client-core's copy has it, such as one just made.
    showing_next: Option<SessionId>,
    pub active: Option<SessionId>,
    /// What the pane holds for each chat.
    chats: HashMap<SessionId, Chat>,
    /// Keeps what's typed and attached in each chat on the hub, as `keeping.rs` says.
    keeper: Drafts,
    /// What's attached in the chat showing, to go with what's typed.
    attached: Attachments,
    waiting: Option<Waiting>,
    /// A new chat is on its way, so another press doesn't make a second.
    creating_chat: bool,
    /// A queued prompt being changed.
    queue_edit: Option<queue::Editing>,
    closed_menu: closed::Menu,
    busy: Busy,
    /// The app is waiting on the hub for a new title for this workspace.
    retitling: bool,
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
    /// Draws the pane again when the line above the composer next says something else of the chat's cache.
    cache_clock: Option<Task<()>>,
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
    /// Where the workspace's work stands, as its mark and the pull request's bar show it.
    pub(crate) fn work(&self) -> look::Work {
        self.pull_request.work()
    }

    /// The pane for the workspace `shown` is part of, as `records` had it.
    pub fn new(
        client: Client,
        records: RecordsFeed,
        keeper: Drafts,
        shown: InWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (composer, on_enter) = composer::new_input(window, cx);
        let on_settings = crate::settings::observe(cx, |this, cx| {
            for transcript in this.every_transcript() {
                transcript.update(cx, |t, cx| t.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx));
            }
        });
        let pull_request = Lookup::recorded(shown.workspace()).unwrap_or(Lookup::Looking);
        // What the hub kept of each chat's composer, from when this app or another last had it.
        let chats = keeping::kept_in(shown.sessions());
        let mut this = Self {
            client,
            records,
            shown,
            showing_next: None,
            active: None,
            chats,
            keeper,
            attached: Attachments::default(),
            waiting: None,
            creating_chat: false,
            queue_edit: None,
            closed_menu: closed::Menu::Shut,
            busy: Busy::Free,
            retitling: false,
            reading: Reading::default(),
            composer,
            diff: None,
            subagents: subagent_tab::Subagents::default(),
            pull_request,
            pull_request_asks: 0,
            setup_log: None,
            setup_log_refresh: None,
            cache_clock: None,
            behind: false,
            hub: composer::Hub::Here,
            window: window.window_handle(),
            subscriptions: vec![on_enter, on_settings],
        };
        // The chat last worked in.
        let latest =
            this.open_sessions().max_by_key(|s| (s.last_active_at_ms(), s.created_at_ms)).map(|s| s.id.clone());
        match latest {
            Some(id) => this.activate(id, cx),
            None => this.ensure_chat(cx),
        }
        this.took_drafts();
        this.bring_back_files(cx);
        this
    }

    pub fn workspace(&self) -> &Workspace {
        self.shown.workspace()
    }

    /// In the order they were made, including closed ones.
    pub fn sessions(&self) -> &[Session] {
        self.shown.sessions()
    }

    fn open_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions().iter().filter(|s| s.is_open())
    }

    pub fn session(&self) -> Option<&Session> {
        self.active.as_ref().and_then(|id| self.shown.session(id))
    }

    pub fn transcript(&self) -> Option<Entity<Transcript>> {
        self.active.as_ref().and_then(|id| self.chats.get(id)?.transcript.as_ref()).map(|(view, _)| view.clone())
    }

    /// What the pane holds for a chat, made the first time it holds anything for it.
    fn held(&mut self, id: &SessionId) -> &mut Chat {
        self.chats.entry(id.clone()).or_default()
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
        self.bring_back_files(cx);
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

    /// What client-core holds for this workspace now. Only what changed since the pane last looked is passed on.
    pub fn set_records(&mut self, shown: InWorkspace, cx: &mut Context<Self>) {
        let earlier = std::mem::replace(&mut self.shown, shown);
        let (id, now) = (self.workspace().id.clone(), self.shown.all().clone());
        let changes = now.changes_since(earlier.all());
        if changes.workspace(&id) {
            if let Some(recorded) = Lookup::recorded(self.workspace()) {
                self.pull_request = recorded;
                self.pass_on_pull_request(cx);
            }
            // Picks up the last lines of the log once setup has finished, while there's a worktree to read it from.
            if self.setup_log.is_some() && self.workspace().status == WorkspaceStatus::Ready {
                self.load_setup_log(cx);
            }
        }
        if changes.sessions(&id) {
            self.set_sessions(&earlier, cx);
        } else if changes.workspace(&id) {
            // Such as the comment being written, changed elsewhere, which a change to the sessions takes too.
            self.take_drafts_from_elsewhere(cx);
        }
        self.pass_on_subagents(&changes, cx);
        self.pass_on_agents(cx);
        if changes.comments(&id)
            && let Some(diff) = &self.diff
        {
            let comments = self.shown.comments().to_vec();
            diff.review.update(cx, |r, cx| r.set_comments(comments, cx));
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

    /// Opens the bar for finding text in the chat showing, or puts focus back in it, as `Cmd-F` does.
    pub fn find_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, |t, cx| t.open_find(window, cx));
        }
    }

    /// Shows the next match in the chat showing, or the one before, as `Cmd-G` and `Cmd-Shift-G` do.
    pub fn find_next(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, |t, cx| t.find_next(forward, window, cx));
        }
    }

    pub fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, Transcript::cancel);
        }
    }
}
