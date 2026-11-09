//! Starting a workspace in one of the hub's repositories, then showing its chat.

use client_core::ClientError;
use client_core::records::{HubRecords, Record};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use protocol::model::{AgentKind, Repo, RepoId};

use super::{MobileApp, Screen};
use crate::chat::ChatView;
use crate::chrome::{self, TAP_HEIGHT};

impl MobileApp {
    pub(super) fn new_workspace_screen(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        let back = Button::new("back")
            .icon(IconName::ChevronLeft)
            .ghost()
            .w(px(TAP_HEIGHT))
            .h(px(TAP_HEIGHT))
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::Workspaces, cx)));
        let mut repos = self.link().and_then(|link| link.read(cx).read(HubRecords::repos)).unwrap_or_default();
        repos.sort_by(|a, b| a.name.cmp(&b.name));
        let body = if repos.is_empty() {
            chrome::empty("No repositories", "Add one on the hub's computer with roundhouse repo add.", cx)
                .into_any_element()
        } else {
            let rows = repos.iter().map(|repo| self.repo_row(repo, cx));
            div().id("repos").flex_1().min_h_0().overflow_y_scroll().children(rows).into_any_element()
        };
        let title = SharedString::from("New workspace");
        v_flex()
            .size_full()
            .child(chrome::header(title, None, Some(back.into_any_element()), None, insets.safe_area.top, cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().px_4().py_2().text_sm().text_color(cx.theme().danger).child(error))
            })
            .child(body)
    }

    fn repo_row(&self, repo: &Repo, cx: &mut Context<Self>) -> AnyElement {
        let creating = self.creating_in.as_ref() == Some(&repo.id);
        let id = repo.id.clone();
        let content = h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .child(div().text_color(cx.theme().muted_foreground).child(if creating {
                IconName::LoaderCircle
            } else {
                IconName::Folder
            }))
            .child(v_flex().flex_1().min_w_0().child(div().truncate().child(repo.name.clone())).child(
                div().text_xs().truncate().text_color(cx.theme().muted_foreground).child(if creating {
                    "Creating…".to_string()
                } else {
                    repo.path.clone()
                }),
            ));
        let button = Button::new(ElementId::Name(format!("repo-{}", repo.id).into()))
            .ghost()
            .w_full()
            .h(px(60.))
            .px_4()
            .child(content)
            .on_click(cx.listener(move |app, _, window, cx| app.create_workspace(id.clone(), window, cx)));
        div().border_b_1().border_color(cx.theme().border).child(button).into_any_element()
    }

    /// Makes a workspace with the hub's own choice of name and branch, as the desktop app does, and opens its chat.
    fn create_workspace(&mut self, repo_id: RepoId, window: &mut Window, cx: &mut Context<Self>) {
        if self.creating_in.is_some() {
            return;
        }
        let Some(client) = self.link().and_then(|link| link.read(cx).client()) else { return };
        self.creating_in = Some(repo_id.clone());
        self.error = None;
        cx.notify();
        self.working = Some(cx.spawn_in(window, async move |this, cx| {
            let for_chat = client.clone();
            let created = cx
                .background_spawn(async move {
                    let workspace =
                        client.workspace_create(&repo_id, None, None, Some(client_core::new_request_id()))?;
                    // Leaves the model and approval mode to the user's own Claude settings.
                    let session = client.session_ensure(&workspace.id, AgentKind::Claude)?;
                    Ok::<_, ClientError>((workspace, session))
                })
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.creating_in = None;
                app.working = None;
                match created {
                    Ok((workspace, session)) => {
                        if let Some(link) = app.link() {
                            link.read(cx).take_reply(Record::Workspace(workspace.clone()));
                            link.read(cx).take_reply(Record::Session(session.clone()));
                        }
                        let chat = cx.new(|cx| ChatView::new(workspace, &[session], for_chat, window, cx));
                        app.show_chat(chat, cx);
                    }
                    Err(e) => {
                        app.error = Some(format!("Couldn't create the workspace: {e}").into());
                        cx.notify();
                    }
                }
            });
        }));
    }
}
