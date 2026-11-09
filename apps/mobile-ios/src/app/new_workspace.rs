//! Starting a workspace in one of the hub's repositories, then showing its chat.

use client_core::ClientError;
use client_core::records::{HubRecords, Record};
use gpui_kit::component::{ActiveTheme as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div,
};
use protocol::model::{Repo, RepoId, Session, Workspace};

use super::{Creating, Followed, MobileApp, Screen};
use crate::chat::ChatView;
use crate::chrome;

impl MobileApp {
    pub(super) fn new_workspace_screen(
        &self,
        followed: &Followed,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = chrome::back_button().on_click(cx.listener(|app, _, _, cx| app.show(Screen::Workspaces, cx)));
        let repos = followed.link.read(cx).read(HubRecords::repos).unwrap_or_default();
        let creating = followed.creating.as_ref().map(|creating| &creating.repo);
        let body = if repos.is_empty() {
            chrome::empty("No repositories", "Add one on the hub's computer with roundhouse repo add.", cx)
        } else {
            let rows = repos.iter().map(|repo| Self::repo_row(repo, creating == Some(&repo.id), cx));
            div().id("repos").flex_1().min_h_0().overflow_y_scroll().children(rows).into_any_element()
        };
        let error = self.error.clone().map(|error| chrome::error_line(error, cx));
        v_flex()
            .size_full()
            .child(chrome::header("New workspace".into(), None, Some(back.into_any_element()), None, window, cx))
            .when_some(error, ParentElement::child)
            .child(body)
            .into_any_element()
    }

    fn repo_row(repo: &Repo, creating: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let content = h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .child(div().text_color(muted).child(if creating { IconName::LoaderCircle } else { IconName::Folder }))
            .child(v_flex().flex_1().min_w_0().child(div().truncate().child(repo.name.clone())).child(
                div().text_xs().truncate().text_color(muted).child(if creating {
                    "Creating…".to_string()
                } else {
                    repo.path.clone()
                }),
            ));
        let id = repo.id.clone();
        let create = cx.listener(move |app, _, window, cx| app.create_workspace(id.clone(), window, cx));
        chrome::row(ElementId::Name(format!("repo-{}", repo.id).into()), content, create, cx)
    }

    /// Makes a workspace with the hub's own choice of name and branch, as the desktop app does, and opens its chat
    /// if this screen is still showing. Otherwise the workspace waits in the list. It's the followed hub's, so
    /// forgetting or replacing the hub stops it.
    fn create_workspace(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(followed) = &mut self.followed else { return };
        let Some(client) = followed.link.read(cx).client() else { return };
        if followed.creating.is_some() {
            return;
        }
        let asked = repo.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let created = cx.background_spawn(async move { client.workspace_start(&asked) }).await;
            let _ = this.update_in(cx, |app, window, cx| app.created(created, window, cx));
        });
        followed.creating = Some(Creating { repo, _task: task });
        self.error = None;
        cx.notify();
    }

    fn created(
        &mut self,
        created: Result<(Workspace, Session), ClientError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
        let Some(followed) = &mut self.followed else { return };
        followed.creating = None;
        let link = followed.link.clone();
        let (workspace, session) = match created {
            Ok(created) => created,
            Err(e) => {
                self.error = Some(format!("Couldn't create the workspace: {e}").into());
                return;
            }
        };
        link.read(cx).take_reply(Record::Workspace(workspace.clone()));
        link.read(cx).take_reply(Record::Session(session.clone()));
        // The connection it was made on may have been replaced since.
        let client = link.read(cx).client();
        if let (Screen::NewWorkspace, Some(client)) = (&self.screen, client) {
            let chat = cx.new(|cx| ChatView::new(workspace, Some(session), client, window, cx));
            self.show_chat(chat, cx);
        }
    }
}
