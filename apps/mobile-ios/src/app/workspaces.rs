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
use std::collections::HashMap;

use protocol::model::{Repo, Session, Workspace, WorkspaceId};

use super::{Followed, MobileApp, Panel};
use crate::chat::ChatView;
use crate::chrome;
use crate::hub_link::{HubLink, listed};

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
            Some(records) => {
                let works = Works::read(&records, link);
                Self::list(&records, &works, ready, window.insets().safe_area.bottom, cx)
            }
        };
        // Such as the vault failing to keep the hub just paired with.
        let error = self.error.clone().map(chrome::error);
        v_flex().size_full().child(header).children(error).child(body).into_any_element()
    }

    /// The repositories by name, as client-core keeps them, and newest first in each.
    fn list(records: &Snapshot, works: &Works, ready: bool, bottom: Pixels, cx: &mut Context<Self>) -> AnyElement {
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
                sections = sections.child(Self::row(workspace, records, works.of(workspace), ready, cx));
            }
        }
        div().id("workspaces").flex_1().min_h_0().w_full().overflow_y_scroll().child(sections).into_any_element()
    }

    fn row(workspace: &Workspace, records: &Snapshot, work: Work, ready: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let signal = client_ui::workspace_signal(workspace, records.sessions(&workspace.id));
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
    pub(super) fn open_workspace(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link() else { return };
        let link = link.read(cx);
        let (Some(client), Some(feed)) = (link.client(), link.feed()) else { return };
        let records = feed.snapshot();
        let Some(workspace) = records.workspace(id).cloned() else { return };
        let session = Session::last_worked_in(records.sessions(id).iter().filter(|s| s.is_open())).cloned();
        let chat = cx.new(|cx| ChatView::new(workspace, session, feed, client, window, cx));
        self.open_chat(chat, window, cx);
    }
}

/// Where each listed workspace's pull request stands, as the hub sent it, for its mark.
struct Works(HashMap<WorkspaceId, Work>);

impl Works {
    fn read(records: &Snapshot, link: &HubLink) -> Self {
        let listed = records.workspaces().iter().filter(|workspace| listed(workspace));
        Self(
            listed
                .filter_map(|workspace| {
                    let work = client_ui::recorded_work(link.pull_request_of(workspace).as_ref())?;
                    Some((workspace.id.clone(), work))
                })
                .collect(),
        )
    }

    fn of(&self, workspace: &Workspace) -> Work {
        self.0.get(&workspace.id).copied().unwrap_or(Work::NoPr)
    }
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
