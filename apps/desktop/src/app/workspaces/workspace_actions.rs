use super::super::AppView;
use super::{in_view, made};
use crate::busy::Busy;
use crate::new_workspace::Request;
use crate::rename;
use crate::rename::Renaming;
use crate::workspace_pane::{PaneEvent, WorkspacePane};
use client_core::ClientError;
use client_core::records::Snapshot;
use gpui_kit::{App, AppContext as _, Context, PathPromptOptions, PromptLevel, Window};
use in_view::InView;
use protocol::client::ErrorCode;
use protocol::model::{PermissionMode, Record, Repo, RepoId, WorkspaceId, WorkspaceStatus};
use protocol::prompt::Prompt;

impl AppView {
    /// Makes the selected workspace's pane if it has none, shows it, and focuses it.
    pub(in super::super) fn make_selected_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        if !self.panes.contains_key(&id) {
            let (Some(client), Some(shown)) = (self.client(), self.records.in_workspace(&id)) else { return };
            // Only for one the sidebar lists, which one being archived is until it's gone.
            if !crate::sidebar::lists(shown.workspace()) {
                return;
            }
            let (Some(records), Some(drafts)) = (self.feed.clone(), self.drafts.clone()) else { return };
            let pane = cx.new(|cx| WorkspacePane::new(client, records, drafts, shown, window, cx));
            let (busy, retitling) = (self.busy.clone(), self.retitling.contains(&id));
            pane.update(cx, |pane, cx| {
                pane.set_busy(busy, cx);
                pane.set_retitling(retitling, cx);
            });
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
                PaneEvent::RegenerateTitle(id) => t.regenerate_title(id, cx),
                PaneEvent::Showing => t.tell_of_needs(cx),
            })
            .detach();
            self.panes.insert(id.clone(), pane);
        }
        if let (Some((_, chat)), Some(pane)) =
            (self.chat_to_show.take_if(|(at, _)| *at == id), self.panes.get(&id).cloned())
        {
            pane.update(cx, |p, cx| p.activate(chat, cx));
        }
        // Only with a connection. Without one, `refresh` catches it up once there is one.
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::shown);
        }
        self.focus_pane(id, window, cx);
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }

    /// The repository a new workspace goes in when nothing says which: the one Home's list shows, while Home shows,
    /// or else the selected workspace's, or the first.
    pub fn current_repo(&self) -> Option<RepoId> {
        let home = self.home_repo().filter(|_| self.home_showing());
        let selected = self.selected.as_ref().and_then(|id| self.records.workspace(id));
        let selected = selected.map(|w| &w.repo_id);
        home.or(selected).or_else(|| self.records.repos().first().map(|r| &r.id)).cloned()
    }

    /// Opens the new-workspace dialog where nothing else says which repository, as `Cmd-N` and Home's and the
    /// sidebar's New do.
    pub(crate) fn new_workspace_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(repo) = self.current_repo() {
            self.open_new_workspace(repo, window, cx);
        }
    }

    pub(crate) fn repo(&self, id: &RepoId) -> Option<&Repo> {
        self.records.repo(id)
    }

    /// Shows the workspace, or brings it back first if it's archived, as clicking its row does. One on its way back
    /// shows once it's there.
    pub(crate) fn open_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        match self.status_of(&id) {
            Some(WorkspaceStatus::Archived) => self.restore(id, cx),
            Some(WorkspaceStatus::Restoring) => {}
            Some(_) | None => self.select(id, cx),
        }
    }

    /// The workspace's status, as its record says.
    pub(super) fn status_of(&self, id: &WorkspaceId) -> Option<WorkspaceStatus> {
        self.records.workspace(id).map(|w| w.status)
    }

    /// Whether a workspace can be made now: there's a hub, and nothing else is being made, archived or restored.
    pub(super) fn can_create(&self) -> bool {
        self.client().is_some() && self.busy.is_free()
    }

    /// Makes a workspace from what the dialog held, and sends its first chat the prompt, if there is one, with the
    /// model, effort and mode chosen. The hub says it's on its way meanwhile, which the sidebar shows, and it shows
    /// once it's made, unless the person has gone elsewhere since. Nothing typed is lost: if the workspace couldn't
    /// be made, the prompt goes back to the dialog, and if it was made and the prompt couldn't be sent, it goes to the
    /// chat's composer, which is asked into plan mode if the prompt was to be planned. Returns the request, untouched,
    /// when a workspace can't be made now.
    pub fn create_workspace(&mut self, request: Request, cx: &mut Context<Self>) -> Result<(), Box<Request>> {
        let Some(client) = self.client().filter(|_| self.can_create()) else { return Err(Box::new(request)) };
        let asked_from = self.in_view();
        self.set_busy(Busy::Creating(request.repo.clone()), cx);
        cx.notify();
        // Kept in case the call never finishes, which takes the files with it. Those still coming back from the hub
        // can be read back again.
        let (typed, coming_back) = (request.prompt.text.clone(), request.coming_back.clone());
        let permission_mode = request.permission_mode;
        let creating = client.start(move |client| Ok(made::make(client, request)));
        cx.spawn(async move |this, cx| {
            let made = creating.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.set_busy(Busy::Free, cx);
                match made {
                    Ok(made) => t.made(made, &asked_from, cx),
                    // Making it stopped partway, so it may have been made.
                    Err(e) => {
                        t.show_error(format!("couldn't tell whether the workspace was made: {e}"), cx);
                        let (prompt, in_the_dialog) = (Prompt::new(typed), false);
                        let given_back = made::GivenBack { prompt, coming_back, in_the_dialog };
                        t.give_back_to_dialog(given_back, permission_mode, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }

    /// What the window shows now, for [`InView::still`] to compare with what it showed when a workspace was asked for.
    pub(super) fn in_view(&self) -> InView {
        InView { selected: self.selected.clone(), main: self.main, settings: self.settings_open() }
    }

    /// Shows the workspace that was made, if the person is still where they were when it was asked for, which was
    /// `asked_from`, and puts back whatever couldn't be sent. Where they went meanwhile stays as it is, since that's
    /// where they chose to be, unless the prompt couldn't be sent, which goes to the new chat's composer for them to
    /// see.
    pub(super) fn made(&mut self, made: made::Made, asked_from: &InView, cx: &mut Context<Self>) {
        let made::Made { workspace, session, unsent, dialog_to_empty } = made;
        if let (Some(made_of), Some(dialog)) = (dialog_to_empty, &self.new_workspace) {
            dialog.update(cx, |dialog, cx| dialog.made_without_the_hub(&made_of, cx));
        }
        let made_id = workspace.as_ref().map(|workspace| workspace.id.clone());
        let chat = session.as_ref().map(|session| session.id.clone());
        if let Some(workspace) = workspace {
            self.take_reply(Record::Workspace(workspace), cx);
        }
        if let Some(session) = session {
            self.take_reply(Record::Session(session), cx);
        }
        let stayed = |id: &WorkspaceId| asked_from.still(&self.in_view(), id);
        if let Some(id) = made_id.clone().filter(|id| stayed(id) || unsent.is_some()) {
            self.select(id, cx);
        }
        let Some(unsent) = unsent else { return };
        self.show_error(unsent.failed.failed(&unsent.error), cx);
        let made::Unsent { given_back, permission_mode, .. } = unsent;
        // To the dialog if nothing was made, or if the hub still keeps it there.
        let in_the_dialog = given_back.as_ref().is_some_and(|given_back| given_back.in_the_dialog);
        let (Some(id), false) = (made_id, in_the_dialog) else {
            if let Some(given_back) = given_back {
                self.give_back_to_dialog(given_back, permission_mode, cx);
            }
            return;
        };
        // To the new chat's composer, or without one, to the first chat the pane shows, which the pane makes itself.
        // After the pane is made, which selecting the workspace put off until just after this. One the hub put in the
        // new chat's composer is there already.
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, |t, cx| match (t.panes.get(&id).cloned(), given_back) {
                (Some(pane), given_back) => pane.update(cx, |pane, cx| {
                    if let Some(made::GivenBack { prompt, .. }) = given_back {
                        pane.give_back(chat.as_ref(), prompt, cx);
                    }
                    if permission_mode == PermissionMode::Plan {
                        pane.plan_given_back(chat.as_ref(), cx);
                    }
                }),
                (None, Some(given_back)) => t.give_back_to_dialog(given_back, permission_mode, cx),
                (None, None) => {}
            });
        });
    }

    pub fn add_repository(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let added = client.start(move |client| client.repo_add(&path.to_string_lossy())).recv().await;
            let _ = this.update(cx, |t, cx| match added {
                Ok(repo) => t.take_reply(Record::Repo(repo), cx),
                Err(e) => t.show_error(format!("couldn't add that: {e}"), cx),
            });
        })
        .detach();
    }

    /// Asks what to call the workspace, and renames it.
    pub fn start_renaming(&mut self, id: WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.records.workspace(&id).map(|w| w.label().to_string()) else {
            return;
        };
        let this = cx.weak_entity();
        rename::open(Renaming::Workspace, &current, window, cx, move |title, cx| {
            let _ = this.update(cx, |t, cx| t.rename(id.clone(), title, cx));
        });
    }

    pub(super) fn rename(&mut self, id: WorkspaceId, title: String, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        let renaming = client.start(move |client| client.workspace_rename(&id, &title));
        cx.spawn(async move |this, cx| {
            let renamed = renaming.recv().await;
            let _ = this.update(cx, |t, cx| match renamed {
                Ok(workspace) => t.take_reply(Record::Workspace(workspace), cx),
                Err(e) => t.show_error(Renaming::Workspace.failed(&e), cx),
            });
        })
        .detach();
    }

    /// Asks the hub to title the workspace again from what's been asked in it, which waits on a model. Its row and its
    /// header say so meanwhile, and asking again before the answer does nothing.
    pub fn regenerate_title(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if !self.retitling.insert(id.clone()) {
            return;
        }
        self.tell_retitling(&id, cx);
        let target = id.clone();
        let asking = client.start(move |client| client.workspace_regenerate_title(&target));
        cx.spawn(async move |this, cx| {
            let retitled = asking.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.retitling.remove(&id);
                t.tell_retitling(&id, cx);
                match retitled {
                    Ok(workspace) => t.take_reply(Record::Workspace(workspace), cx),
                    Err(e) => t.show_error(rename::regenerating_failed(&e), cx),
                }
            });
        })
        .detach();
    }

    /// Tells the workspace's pane, if it has one, whether a new title is on its way.
    pub(super) fn tell_retitling(&self, id: &WorkspaceId, cx: &mut Context<Self>) {
        let retitling = self.retitling.contains(id);
        if let Some(pane) = self.panes.get(id) {
            pane.update(cx, |pane, cx| pane.set_retitling(retitling, cx));
        }
        cx.notify();
    }

    /// Archives, and asks before losing uncommitted work. Does nothing while the app is making, archiving or restoring
    /// a workspace, so a key pressed twice archives once, nor for one that isn't ready, such as one already on its way
    /// out, whoever asked. Its record says it's being archived from the moment the hub has the request.
    pub fn archive(&mut self, id: WorkspaceId, force: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if !self.busy.is_free() || self.status_of(&id) != Some(WorkspaceStatus::Ready) {
            return;
        }
        self.set_busy(Busy::Moving, cx);
        let target = id.clone();
        let archiving = client.start(move |client| client.workspace_archive(&target, force));
        cx.spawn(async move |this, cx| {
            let result = archiving.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.set_busy(Busy::Free, cx);
                match result {
                    Ok(workspace) => t.take_reply(Record::Workspace(workspace), cx),
                    Err(ClientError::Api(e)) if e.code == ErrorCode::NeedsForce && !force => {
                        t.confirm_forced_archive(id.clone(), &e.message, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't archive: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn confirm_forced_archive(&mut self, id: WorkspaceId, reason: &str, cx: &mut Context<Self>) {
        let handle = self.window;
        let answer = handle.update(cx, |_, window, cx| {
            window.prompt(
                PromptLevel::Warning,
                "Archive anyway?",
                Some(reason),
                &["Keep working", "Archive anyway"],
                cx,
            )
        });
        let Ok(answer) = answer else { return };
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |t, cx| t.archive(id, true, cx));
            }
        })
        .detach();
    }

    /// Restores an archived workspace. Does nothing while the app is making, archiving or restoring a workspace, nor
    /// for one that isn't archived, as [`AppView::archive`] doesn't.
    pub fn restore(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if !self.busy.is_free() || self.status_of(&id) != Some(WorkspaceStatus::Archived) {
            return;
        }
        self.set_busy(Busy::Moving, cx);
        let restoring = client.start(move |client| client.workspace_restore(&id));
        cx.spawn(async move |this, cx| {
            let result = restoring.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.set_busy(Busy::Free, cx);
                match result {
                    Ok(workspace) => {
                        let id = workspace.id.clone();
                        t.take_reply(Record::Workspace(workspace), cx);
                        t.select(id, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't restore: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Says whether the app is making, archiving or restoring a workspace, which greys out every button that archives:
    /// in the sidebar, in Home, and on every pane's pull request bar. What the one being archived or restored says in
    /// place of its button is its record's to say.
    pub(super) fn set_busy(&mut self, busy: Busy, cx: &mut Context<Self>) {
        for pane in self.panes.values() {
            pane.update(cx, |pane, cx| pane.set_busy(busy.clone(), cx));
        }
        self.busy = busy;
        cx.notify();
    }

    /// Lets go of the panes of workspaces that have gone or been archived. A pane's chats close with it, without
    /// closing one by one, so it lets go of their subagents' records itself.
    pub(in super::super) fn drop_hidden_panes(&mut self, records: &Snapshot, cx: &App) {
        self.panes.retain(|id, pane| {
            let shown = records.workspace(id).is_some_and(|w| w.status != WorkspaceStatus::Archived);
            if !shown {
                pane.read(cx).let_go_of_subagents();
            }
            shown
        });
    }
}
