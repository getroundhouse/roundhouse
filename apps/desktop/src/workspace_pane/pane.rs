use super::{attaching, closed, composer, dictating, file_tab, proposal_tab, queue, reading, subagent_tab, terminals};
use crate::all_files::AllFiles;
use crate::attached::Attachments;
use crate::busy::Busy;
use crate::changes::Changes;
use crate::file_diff::FileDiff;
use crate::file_view::FileView;
use crate::pull_request::Lookup;
use crate::review::Review;
use crate::sidebar_tabs::TabShown;
use crate::warnings::Warning;
use attaching::{Composed, Waiting};
use client_core::records::InWorkspace;
use client_core::{Client, Drafts, RecordsFeed};
use conversation::Conversations;
use file_tab::FileTab;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{AnyWindowHandle, Context, Entity, EventEmitter, Subscription, Task, Window};
use protocol::attachment::Attachment;
use protocol::model::{AgentKind, Seq, Session, SessionId, TerminalId, Workspace, WorkspaceId, WorkspaceStatus};
use reading::Reading;
use std::collections::{HashMap, VecDeque};
use transcript::Transcript;

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
    /// One of the workspace's terminals, in place of the active chat.
    Terminal,
    /// The proposal tab, in place of the active chat.
    Proposal,
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
    pub(super) sidebar_tab: TabShown,
    /// The file tab, while it's open. It's here so there's never a file tab without a view to show in it.
    pub(super) tab: Option<FileTab>,
    /// A turn ended while the pane wasn't on screen, so the files are listed again once it is.
    pub(super) files_behind: bool,
}

/// What the pane holds for one chat, from the first time it holds anything for it until the chat closes, when it all
/// goes at once.
#[derive(Default)]
pub(super) struct Chat {
    /// Made the first time the chat is shown, then kept so it stays current and keeps its place.
    pub(super) transcript: Option<(Entity<Transcript>, Subscription)>,
    /// What's been typed and attached in it while it isn't showing.
    pub(super) draft: Option<Composed>,
    /// The files the hub kept with its draft that haven't been read back yet.
    pub(super) restoring: Option<Vec<Attachment>>,
    /// What Shift-Tab last asked, whether to plan, while the hub hasn't answered yet.
    pub(super) plan_asked: Option<bool>,
    /// Its newest reply, while it's being marked read. It shows as read meanwhile, so a reply that comes while it's
    /// showing doesn't flash as unread first.
    pub(super) marking: Option<Seq>,
}

/// A pane asks its owning hub view to present window effects, so replies from an unselected hub cannot show a
/// modal over another hub. The callback keeps a weak pane handle, since its workspace may have gone meanwhile.
type Presentation = std::rc::Rc<dyn Fn(&mut Window, &mut gpui_kit::App)>;

#[derive(Clone)]
pub enum PaneEvent {
    Error(String),
    Archive(WorkspaceId, bool),
    /// Title the workspace again from what's been asked in it.
    RegenerateTitle(WorkspaceId),
    /// What's in front under the tabs changed, so the chat being read may have, and what has focus should follow it
    /// in the pane on screen.
    Showing,
    Present(Presentation),
    /// Show Git in Settings, where the default GitHub account is chosen and accounts are connected.
    OpenGit,
    /// Show another workspace, as the menu of a pull request a chat links to asks.
    GoToWorkspace(WorkspaceId),
}

/// A tab to show once client-core's copy of the records has it open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Next {
    Chat(SessionId),
    Terminal(TerminalId),
}

pub struct WorkspacePane {
    pub(super) model_lookup: client_ui::FollowedModels,
    pub(super) client: Client,
    /// Client-core's copy of the hub's records, which keeps the subagents of each chat the pane shows.
    pub(super) records: RecordsFeed,
    /// What's shown of the workspace's records, as of client-core's last change: its own, its sessions' and its
    /// comments', and through them the accounts its sessions name and their subagents.
    pub(super) shown: InWorkspace,
    /// A chat or a terminal to show once client-core's copy has it, such as one just made. Only the last whose reply
    /// came, so one whose record comes after a newer one's doesn't take the front from it.
    pub(super) showing_next: Option<Next>,
    pub active: Option<SessionId>,
    /// What the pane holds for each chat.
    pub(super) chats: HashMap<SessionId, Chat>,
    /// Keeps what's typed and attached in each chat on the hub, as `keeping.rs` says.
    pub(super) keeper: Drafts,
    /// What's attached in the chat showing, to go with what's typed.
    pub(super) attached: Attachments,
    pub(super) waiting: Option<Waiting>,
    /// A new chat is on its way, so a workspace with none joins it rather than making a second.
    pub(super) creating_chat: bool,
    /// The agents of the chats asked for while one was on its way, oldest first. Each is made once the one before it
    /// is, so they open in the order they were asked for.
    pub(super) chats_asked_for: VecDeque<AgentKind>,
    /// A queued prompt being changed.
    pub(super) queue_edit: Option<queue::Editing>,
    pub(super) closed_menu: closed::Menu,
    pub(super) busy: Busy,
    /// The app is waiting on the hub for a new title for this workspace.
    pub(super) retitling: bool,
    pub(super) reading: Reading,
    /// What's being dictated into the composer, which ends with the pane.
    pub(super) dictation: Entity<dictation_ui::Field>,
    pub(super) dictation_chat: Option<SessionId>,
    /// The chat the person started talking in, in conversation mode, and what the composer held then, which isn't part
    /// of what they say, and is put back once it's sent.
    pub(super) typed_before_talking: Option<(SessionId, String)>,
    /// The hub's chats' conversations on this Mac, which the app follows whichever workspace shows.
    pub(super) conversations: Entity<Conversations>,
    /// The chat whose conversation mode is waiting on dictation and speech, while its card shows how they stand. The
    /// mode starts once both are ready, unless the card has closed.
    pub(super) setting_up: Option<super::setting_up::SettingUp>,
    pub(super) after_dictation: dictation_ui::Pending<dictating::Then>,
    pub composer: Entity<TextareaState>,
    pub diff: Option<Diff>,
    pub(super) subagents: subagent_tab::Subagents,
    pub(super) terminals: terminals::Terminals,
    pub(super) proposals: proposal_tab::Proposals,
    pub pull_request: Lookup,
    /// What's wrong that the workspace can't put right itself, which the card its name opens says, as the hub last
    /// said.
    pub(super) warnings: Vec<Warning>,
    /// How many times the pull request has been asked for, so only the newest answer is shown.
    pub setup_log: Option<String>,
    /// Keeps the open log current while setup runs.
    pub(super) setup_log_refresh: Option<Task<()>>,
    /// Draws the pane again when the line above the composer next says something else of the chat's cache.
    pub(super) cache_clock: Option<Task<()>>,
    /// Set on a new connection, until [`Self::catch_up`] reads again what it may have changed.
    pub(super) behind: bool,
    /// Away while the app connects to a hub again, with this still on screen. `composer.rs` has what that holds up.
    pub(super) hub: composer::Hub,
    pub(super) window: AnyWindowHandle,
    pub(super) subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaneEvent> for WorkspacePane {}

impl WorkspacePane {
    /// The pane for the workspace `shown` is part of, as `records` had it.
    pub fn new(
        client: Client,
        records: RecordsFeed,
        keeper: Drafts,
        conversations: Entity<Conversations>,
        shown: InWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (composer, on_enter) = composer::new_input(window, cx);
        let (hints, on_event) = (Self::composer_hints, Self::dictation_event);
        let (dictation, [dictated, live]) =
            dictation_ui::Field::attach(&composer, look::Corner::Card, true, hints, on_event, window, cx);
        let on_settings = crate::settings::observe(cx, |this, cx| {
            for transcript in this.every_transcript() {
                transcript.update(cx, |t, cx| t.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx));
            }
        });
        // The microphone is as dictation stands.
        let on_dictation = crate::dictating::observe(cx, |_, cx| cx.notify());
        let [needs_dictation, needs_speech] = crate::conversing::observe_needs(window, cx, Self::start_once_ready);
        let (pull_request, warnings) = Self::known(&records.github(), shown.workspace());
        let mut this = Self {
            client,
            records,
            shown,
            showing_next: None,
            active: None,
            chats: HashMap::new(),
            keeper,
            attached: Attachments::default(),
            waiting: None,
            creating_chat: false,
            chats_asked_for: VecDeque::new(),
            queue_edit: None,
            closed_menu: closed::Menu::Shut,
            busy: Busy::Free,
            retitling: false,
            reading: Reading::default(),
            model_lookup: client_ui::FollowedModels::default(),
            dictation,
            dictation_chat: None,
            typed_before_talking: None,
            conversations,
            setting_up: None,
            after_dictation: dictation_ui::Pending::default(),
            composer,
            diff: None,
            subagents: subagent_tab::Subagents::default(),
            terminals: terminals::Terminals::default(),
            proposals: proposal_tab::Proposals::default(),
            pull_request,
            warnings,
            setup_log: None,
            setup_log_refresh: None,
            cache_clock: None,
            behind: false,
            hub: composer::Hub::Here,
            window: window.window_handle(),
            subscriptions: vec![on_enter, on_settings, on_dictation, needs_dictation, needs_speech, dictated, live],
        };
        this.take_kept_drafts(cx);
        // The chat last worked in.
        let latest = Session::last_worked_in(this.open_sessions()).map(|s| s.id.clone());
        match latest {
            Some(id) => this.activate(id, cx),
            None => this.ensure_chat(cx),
        }
        this
    }

    pub fn workspace(&self) -> &Workspace {
        self.shown.workspace()
    }

    /// In the order they were made, including closed ones.
    pub fn sessions(&self) -> &[Session] {
        self.shown.sessions()
    }

    pub(super) fn open_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions().iter().filter(|s| s.is_open())
    }

    pub fn session(&self) -> Option<&Session> {
        self.active.as_ref().and_then(|id| self.shown.session(id))
    }

    pub fn transcript(&self) -> Option<Entity<Transcript>> {
        self.active.as_ref().and_then(|id| self.chats.get(id)?.transcript.as_ref()).map(|(view, _)| view.clone())
    }

    /// What the pane holds for a chat, made the first time it holds anything for it.
    pub(super) fn held(&mut self, id: &SessionId) -> &mut Chat {
        self.chats.entry(id.clone()).or_default()
    }

    /// Carries on on a new connection to the hub, keeping everything shown and typed, and reads the chat showing if
    /// it's being read. What the hub reads from the worktree waits for [`Self::catch_up`], so a reconnect doesn't
    /// start a worker for every pane at once.
    pub fn reconnect(&mut self, client: &Client, cx: &mut Context<Self>) {
        self.model_lookup.reset();
        self.client = client.clone();
        self.hub = composer::Hub::Here;
        self.showing_changed(cx);
        for transcript in self.every_transcript() {
            transcript.update(cx, |t, cx| t.reconnect(client, cx));
        }
        self.reconnect_terminals(client, cx);
        if let Some(view) = self.proposals.view() {
            view.update(cx, |v, cx| v.reconnect(client, cx));
        }
        if let Some(diff) = &mut self.diff {
            diff.sidebar_tab.reconnected();
            diff.review.update(cx, |r, cx| r.reconnect(client, cx));
            diff.all_files.update(cx, |a, _| a.reconnect(client));
            diff.viewer.update(cx, |v, cx| v.reconnect(client, cx));
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

    /// For each time the pane is shown. Reads again what the sidebar of changes and the file tab show of the worktree,
    /// and takes its pull request as the hub has sent it. The hub reads that as the app shows it.
    pub fn shown(&mut self, cx: &mut Context<Self>) {
        self.behind = false;
        self.take_pull_request(cx);
        self.load_changes(cx);
    }

    /// What client-core holds for this workspace now. Only what changed since the pane last looked is passed on.
    pub fn set_records(&mut self, shown: InWorkspace, cx: &mut Context<Self>) {
        let earlier = std::mem::replace(&mut self.shown, shown);
        let (id, now) = (self.workspace().id.clone(), self.shown.all().clone());
        let changes = now.changes_since(earlier.all());
        // Each transcript takes what changed of its chat's records, before the pane decides which chats and tabs stay.
        for transcript in self.every_transcript() {
            transcript.update(cx, |t, cx| t.records_changed(&now, cx));
        }
        if changes.workspace(&id) {
            self.take_pull_request(cx);
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
        self.name_the_tab_s_subagent();
        self.pass_on_proposals(&changes, cx);
        if changes.terminals(&id) {
            self.set_terminals(cx);
        }
        // Once the sessions and the terminals are both taken in, whichever the next tab is.
        self.show_next(cx);
        self.pass_on_agents(cx);
        self.follow_models(false, cx);
        if changes.workspace_changes(&id) {
            self.take_changed_files(cx);
        }
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
    pub(super) fn with_window(
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
