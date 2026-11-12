//! Adding repositories, and making, renaming, retitling, archiving and restoring workspaces.

use client_core::ClientError;
use client_core::records::Record;
use gpui_kit::{App, Context, PathPromptOptions, PromptLevel, Window};
use protocol::client::ErrorCode;
use protocol::model::{PermissionMode, Repo, RepoId, Workspace, WorkspaceId, WorkspaceStatus};

use super::AppView;
use crate::busy::Busy;
use crate::new_workspace::Request;
use crate::rename::{self, Renaming};

mod dialog;
mod made;

impl AppView {
    /// The repository a new workspace goes in when nothing says which: the one Home's list shows, while Home shows,
    /// or else the selected workspace's, or the first.
    pub fn current_repo(&self) -> Option<RepoId> {
        let home = self.home_repo().filter(|_| self.home_showing());
        let selected = self.selected.as_ref().and_then(|id| self.workspaces.iter().find(|w| &w.id == id));
        let selected = selected.map(|w| &w.repo_id);
        home.or(selected).or_else(|| self.repos.first().map(|r| &r.id)).cloned()
    }

    /// Opens the new-workspace dialog where nothing else says which repository, as `Cmd-N` and Home's and the
    /// sidebar's New do.
    pub(crate) fn new_workspace_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(repo) = self.current_repo() {
            self.open_new_workspace(repo, window, cx);
        }
    }

    pub(crate) fn repo(&self, id: &RepoId) -> Option<&Repo> {
        self.repos.iter().find(|r| &r.id == id)
    }

    /// Shows the workspace, or brings it back first if it's archived, as clicking its row does.
    pub(crate) fn open_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let archived = self.workspaces.iter().any(|w| w.id == id && w.status == WorkspaceStatus::Archived);
        if archived {
            self.restore(id, cx);
        } else {
            self.select(id, cx);
        }
    }

    /// Whether a workspace can be made now: there's a hub, and nothing else is being made, archived or restored.
    fn can_create(&self) -> bool {
        self.client().is_some() && self.busy.is_free()
    }

    /// Makes a workspace from what the dialog held, and sends its first chat the prompt, if there is one, with the
    /// model, effort and mode chosen. The sidebar says it's being made meanwhile. Nothing typed is lost: if the
    /// workspace couldn't be made, the prompt goes back to the dialog, and if it was made and the prompt couldn't be
    /// sent, it goes to the chat's composer, which is asked into plan mode if the prompt was to be planned. Returns the
    /// request, untouched, when a workspace can't be made now.
    pub fn create_workspace(&mut self, request: Request, cx: &mut Context<Self>) -> Result<(), Request> {
        let Some(client) = self.client().filter(|_| self.can_create()) else { return Err(request) };
        self.sidebar.making_among(&self.workspaces);
        self.set_busy(Busy::Creating(request.repo.clone()), cx);
        cx.notify();
        // Kept in case the call never finishes, which takes the files with it.
        let (typed, permission_mode) = (request.prompt.clone(), request.permission_mode);
        let creating = client.start(move |client| Ok(made::make(client, request)));
        cx.spawn(async move |this, cx| {
            let made = creating.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.set_busy(Busy::Free, cx);
                match made {
                    Ok(made) => t.made(made, cx),
                    // Making it stopped partway, so it may have been made.
                    Err(e) => {
                        t.show_error(format!("couldn't tell whether the workspace was made: {e}"), cx);
                        t.give_back_to_dialog(typed, Vec::new(), permission_mode, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }

    /// Shows the workspace that was made, and puts back whatever couldn't be sent.
    fn made(&mut self, made: made::Made, cx: &mut Context<Self>) {
        let made::Made { workspace, session, unsent } = made;
        let shown = workspace.as_ref().map(|workspace| workspace.id.clone());
        let chat = session.as_ref().map(|session| session.id.clone());
        if let Some(workspace) = workspace {
            self.take_reply(Record::Workspace(workspace), cx);
        }
        if let Some(session) = session {
            self.take_reply(Record::Session(session), cx);
        }
        if let Some(id) = shown.clone() {
            self.select(id, cx);
        }
        let Some(unsent) = unsent else { return };
        self.show_error(unsent.failed.failed(&unsent.error), cx);
        let Some(id) = shown else {
            self.give_back_to_dialog(unsent.text, unsent.files, unsent.permission_mode, cx);
            return;
        };
        // To the new chat's composer, or without one, to the first chat the pane shows, which the pane makes itself.
        // After the pane is made, which selecting the workspace put off until just after this.
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, |t, cx| match t.panes.get(&id).cloned() {
                Some(pane) => pane.update(cx, |pane, cx| {
                    pane.give_back(chat.as_ref(), unsent.text, unsent.files, cx);
                    if unsent.permission_mode == PermissionMode::Plan {
                        pane.plan_given_back(chat.as_ref(), cx);
                    }
                }),
                None => t.give_back_to_dialog(unsent.text, unsent.files, unsent.permission_mode, cx),
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
        let Some(current) = self.workspaces.iter().find(|w| w.id == id).map(|w| w.label().to_string()) else {
            return;
        };
        let this = cx.weak_entity();
        rename::open(Renaming::Workspace, &current, window, cx, move |title, cx| {
            let _ = this.update(cx, |t, cx| t.rename(id.clone(), title, cx));
        });
    }

    fn rename(&mut self, id: WorkspaceId, title: String, cx: &mut Context<Self>) {
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
    fn tell_retitling(&self, id: &WorkspaceId, cx: &mut Context<Self>) {
        let retitling = self.retitling.contains(id);
        if let Some(pane) = self.panes.get(id) {
            pane.update(cx, |pane, cx| pane.set_retitling(retitling, cx));
        }
        cx.notify();
    }

    /// Archives, and asks before losing uncommitted work. Does nothing while a workspace is being made, archived or
    /// restored, so a key pressed twice archives once.
    pub fn archive(&mut self, id: WorkspaceId, force: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if !self.busy.is_free() {
            return;
        }
        self.set_busy(Busy::Archiving(id.clone()), cx);
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

    fn confirm_forced_archive(&mut self, id: WorkspaceId, reason: &str, cx: &mut Context<Self>) {
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

    /// Restores an archived workspace. Does nothing while a workspace is being made, archived or restored, as
    /// [`AppView::archive`] doesn't.
    pub fn restore(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if !self.busy.is_free() {
            return;
        }
        self.set_busy(Busy::Restoring(id.clone()), cx);
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

    /// Says whether a workspace is being made, archived or restored, which greys out every button that archives: in
    /// the sidebar, in Home, and on every pane's pull request bar. The one it's for says so in place of its button.
    fn set_busy(&mut self, busy: Busy, cx: &mut Context<Self>) {
        for pane in self.panes.values() {
            pane.update(cx, |pane, cx| pane.set_busy(busy.clone(), cx));
        }
        self.busy = busy;
        cx.notify();
    }

    /// Lets go of the panes of workspaces that have gone or been archived. A pane's chats close with it, without
    /// closing one by one, so it lets go of their subagents' records itself.
    pub(super) fn drop_hidden_panes(&mut self, workspaces: &[Workspace], cx: &App) {
        self.panes.retain(|id, pane| {
            let shown = workspaces.iter().any(|w| &w.id == id && w.status != WorkspaceStatus::Archived);
            if !shown {
                pane.read(cx).let_go_of_subagents();
            }
            shown
        });
    }
}
