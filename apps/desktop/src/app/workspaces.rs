//! Adding repositories, and making, archiving and restoring workspaces.

use client_core::ClientError;
use client_core::records::Record;
use gpui_kit::{AppContext, Context, PathPromptOptions, PromptLevel};
use protocol::client::ErrorCode;
use protocol::model::{AgentKind, RepoId, WorkspaceId};

use super::AppView;

impl AppView {
    pub fn new_workspace(&mut self, repo_id: RepoId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        if self.busy {
            return;
        }
        self.busy = true;
        self.creating_in = Some(repo_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let created = cx
                .background_spawn(async move {
                    let workspace =
                        client.workspace_create(&repo_id, None, None, Some(client_core::new_request_id()))?;
                    // Leaves the model and approval mode to the user's own Claude settings.
                    let session = client.session_ensure(&workspace.id, AgentKind::Claude)?;
                    Ok::<_, ClientError>((workspace, session))
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
                t.creating_in = None;
                match created {
                    Ok((workspace, session)) => {
                        let id = workspace.id.clone();
                        t.take_reply(Record::Workspace(workspace), cx);
                        t.take_reply(Record::Session(session), cx);
                        t.select(id, cx);
                    }
                    Err(e) => t.show_error(format!("couldn't create the workspace: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
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
            let added = cx.background_spawn(async move { client.repo_add(&path.to_string_lossy()) }).await;
            let _ = this.update(cx, |t, cx| match added {
                Ok(repo) => t.take_reply(Record::Repo(repo), cx),
                Err(e) => t.show_error(format!("couldn't add that: {e}"), cx),
            });
        })
        .detach();
    }

    /// Archives, and asks before losing uncommitted work.
    pub fn archive(&mut self, id: WorkspaceId, force: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let target = id.clone();
            let result = cx.background_spawn(async move { client.workspace_archive(&target, force) }).await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
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

    pub fn restore(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { client.workspace_restore(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                t.busy = false;
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
}
