//! Starting a workspace in one of the hub's repositories, then opening its chat.

use client_core::ClientError;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use look::{IconSize, IconSizing as _, RowSize, Type, Typeset as _};
use protocol::client::new_workspace::NewWorkspaceMade;
use protocol::model::{NewWorkspaceDraftToKeep, Repo, RepoId};

use super::{Creating, Followed, MobileApp};
use crate::chat::ChatView;
use crate::chrome;

impl MobileApp {
    pub(super) fn new_workspace_screen(
        &self,
        followed: &Followed,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let back = chrome::back().on_click(cx.listener(|app, _, _, cx| app.show_panel(None, cx)));
        let repos = followed.link.read(cx).snapshot().map_or_default(|records| records.repos().to_vec());
        let creating = followed.creating.as_ref().map(|creating| &creating.repo);
        let body = if repos.is_empty() {
            chrome::empty("No repositories", "Add one on the hub's computer, with roundhouse repo add.")
        } else {
            let rows = repos.iter().map(|repo| Self::repo_row(repo, creating, cx));
            let list = v_flex().px(look::ROWS_INSET).pb(window.insets().safe_area.bottom).children(rows);
            div().id("repos").flex_1().min_h_0().overflow_y_scroll().child(list).into_any_element()
        };
        let error = self.error.clone().map(chrome::error);
        v_flex()
            .size_full()
            .child(chrome::header("New workspace".into(), None, Some(back), Vec::new(), window, cx))
            .children(error)
            .child(body)
            .into_any_element()
    }

    /// A repository, which starts a workspace in it, and says so while it does. Each waits while one is being made.
    fn repo_row(repo: &Repo, creating: Option<&RepoId>, cx: &mut Context<Self>) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let this_one = creating == Some(&repo.id);
        let icon = if this_one { IconName::LoaderCircle } else { IconName::Folder };
        let at = repo
            .origin
            .checkout()
            .or_else(|| repo.origin.url().map(protocol::model::RepoUrl::as_str))
            .unwrap_or_default();
        let under = if this_one { "Starting a workspace…" } else { at }.to_string();
        let names = v_flex()
            .flex_1()
            .min_w_0()
            .child(div().typeset(Type::Body).truncate().child(repo.name.clone()))
            .child(div().typeset(Type::Detail).text_color(muted).truncate().child(under));
        let id = repo.id.clone();
        look::list_row(ElementId::Name(format!("repo-{}", repo.id).into()), RowSize::Large)
            .off(creating.is_some())
            .child(Icon::new(icon).sized(IconSize::Large).text_color(muted))
            .child(names)
            .on_click(cx.listener(move |app, _, window, cx| app.create_workspace(id.clone(), window, cx)))
            .into_any_element()
    }

    /// Has the hub make a workspace with its own choice of name and branch, and its first chat on the first agent it
    /// describes, as the desktop app's does with nothing typed, then opens the chat if this screen is still showing.
    /// Otherwise the workspace waits in the list. It's the followed hub's, so forgetting or replacing the hub stops it.
    fn create_workspace(&mut self, repo: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(followed) = &mut self.followed else { return };
        let Some(client) = followed.link.read(cx).client() else { return };
        if followed.creating.is_some() {
            return;
        }
        let made_of = NewWorkspaceDraftToKeep { repo_id: Some(repo.clone()), ..NewWorkspaceDraftToKeep::default() };
        let made = client.start(move |client| client.new_workspace_create(&made_of, client_core::new_request_id()));
        let task = cx.spawn_in(window, async move |this, cx| {
            let made = made.recv().await;
            let _ = this.update_in(cx, |app, window, cx| app.created(made, window, cx));
        });
        followed.creating = Some(Creating { repo, _task: task });
        self.error = None;
        cx.notify();
    }

    fn created(&mut self, made: Result<NewWorkspaceMade, ClientError>, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        let Some(followed) = &mut self.followed else { return };
        followed.creating = None;
        let link = followed.link.clone();
        let NewWorkspaceMade { workspace, session, .. } = match made {
            Ok(made) => made,
            Err(e) => {
                self.error = Some(format!("Couldn't start the workspace: {e}").into());
                return;
            }
        };
        // The connection it was made on may have been replaced since.
        let (client, feed) = (link.read(cx).client(), link.read(cx).feed());
        if let (Some(super::Panel::NewWorkspace), Some(client), Some(feed)) = (self.open.panel, client, feed) {
            let chat = cx.new(|cx| ChatView::new(workspace, Some(session), feed, client, window, cx));
            self.open_chat(chat, cx);
        }
    }
}
