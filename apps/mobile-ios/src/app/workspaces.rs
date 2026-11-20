//! The hub's workspaces, grouped by repository, each with its mark, and opening one's chat.

use client_core::records::Snapshot;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use look::{IconSize, IconSizing as _, RowSize, Space, Type, Typeset as _, Work};
use protocol::model::{Repo, Session, Workspace, WorkspaceId, WorkspaceStatus};

use super::{Followed, MobileApp, Panel};
use crate::chat::ChatView;
use crate::chrome;

/// A workspace's row: its name and its branch under it, which takes a large row.
const ROW: RowSize = RowSize::Large;

impl MobileApp {
    pub(super) fn workspaces_screen(&self, followed: &Followed, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let link = followed.link.read(cx);
        let (name, ready) = (link.hub().hub_name.clone(), link.client().is_some());
        let hub = chrome::icon_button("hub", IconName::Server, "Hub")
            .on_click(cx.listener(|app, _, _, cx| app.show_panel(Some(Panel::Hub), cx)));
        let new = chrome::icon_button("new-workspace", IconName::Plus, "New workspace")
            .off(!ready)
            .on_click(cx.listener(|app, _, _, cx| app.show_panel(Some(Panel::NewWorkspace), cx)));
        let header = chrome::header(
            name.clone().into(),
            Some(chrome::connection(link.state())),
            None,
            vec![hub, new],
            window,
            cx,
        );
        let body = match link.snapshot() {
            None => chrome::empty(format!("Connecting to {name}"), "The workspaces show once it's connected."),
            Some(records) if !records.workspaces().iter().any(listed) => {
                chrome::empty("No workspaces", "Tap + to start one.")
            }
            Some(records) => Self::list(&records, ready, window.insets().safe_area.bottom, cx),
        };
        // Such as the vault failing to keep the hub just paired with.
        let error = self.error.clone().map(chrome::error);
        v_flex().size_full().child(header).children(error).child(body).into_any_element()
    }

    /// The repositories by name, as client-core keeps them, and newest first in each.
    fn list(records: &Snapshot, ready: bool, bottom: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let mut sections = v_flex().w_full().px(look::ROWS_INSET).pb(bottom);
        for repo in records.repos() {
            let mut workspaces: Vec<&Workspace> =
                records.workspaces().iter().filter(|w| w.repo_id == repo.id && listed(w)).collect();
            if workspaces.is_empty() {
                continue;
            }
            workspaces.sort_by_key(|w| std::cmp::Reverse(w.created_at_ms));
            sections = sections.child(heading(repo, muted));
            for workspace in workspaces {
                sections = sections.child(Self::row(workspace, records, ready, cx));
            }
        }
        div().id("workspaces").flex_1().min_h_0().w_full().overflow_y_scroll().child(sections).into_any_element()
    }

    fn row(workspace: &Workspace, records: &Snapshot, ready: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let signal = transcript::workspace_signal(workspace, records.sessions(&workspace.id));
        let work = transcript::recorded_work(workspace).unwrap_or(Work::NoPr);
        let id = workspace.id.clone();
        let names = v_flex()
            .flex_1()
            .min_w_0()
            .child(div().typeset(Type::Body).truncate().child(workspace.label().to_string()))
            .when_some(chrome::branch_under_name(workspace), |d, branch| {
                d.child(div().typeset(Type::Detail).text_color(muted).truncate().child(branch))
            });
        look::list_row(ElementId::Name(format!("workspace-{}", workspace.id).into()), ROW)
            .off(!ready)
            .child(look::mark(signal, work, look::Size::Row, cx))
            .child(names)
            .child(Icon::new(IconName::ChevronRight).sized(IconSize::Regular).text_color(muted))
            .on_click(cx.listener(move |app, _, window, cx| app.open_workspace(&id, window, cx)))
            .into_any_element()
    }

    /// Opens a workspace's chat: the one last worked in, or with none open, the one the hub finds or starts. Needs a
    /// connection, since the chat reads its history from the hub.
    fn open_workspace(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link() else { return };
        let link = link.read(cx);
        let (Some(client), Some(feed)) = (link.client(), link.feed()) else { return };
        let records = feed.snapshot();
        let Some(workspace) = records.workspace(id).cloned() else { return };
        let session = Session::last_worked_in(records.sessions(id).iter().filter(|s| s.is_open())).cloned();
        let chat = cx.new(|cx| ChatView::new(workspace, session, feed, client, window, cx));
        self.open_chat(chat, cx);
    }
}

/// Whether the list shows it: ready ones only, since one still being made, being archived or archived has nothing to
/// open here. The desktop's sidebar also lists one being archived, and notes one being made.
fn listed(workspace: &Workspace) -> bool {
    workspace.status == WorkspaceStatus::Ready
}

/// A repository's name over its workspaces.
fn heading(repo: &Repo, muted: gpui_kit::Hsla) -> AnyElement {
    div()
        .h(RowSize::Small.height())
        .flex()
        .items_end()
        .px(Space::Medium)
        .pb(Space::XSmall)
        .typeset(Type::Detail)
        .emphasis()
        .text_color(muted)
        .child(repo.name.clone())
        .into_any_element()
}
