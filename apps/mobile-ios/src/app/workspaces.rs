//! The hub's workspaces, grouped by repository, each with its mark, and opening one's chat.

use client_core::records::Snapshot;
use client_core::workspace_order::Sort;
use gpui_kit::assets::IconName;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use look::{RowSize, Space, Type, Typeset as _, Work};
use protocol::model::{Repo, Session, Workspace, WorkspaceId};
use std::collections::HashMap;

use super::{Followed, MobileApp, Panel};
use crate::chat::ChatView;
use crate::chrome;
use crate::hub_link::{HubLink, listed};

/// A workspace's row: its mark and its name, as the desktop's sidebar has them, in a list's regular row.
const ROW: RowSize = RowSize::Regular;

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
                self.list(&records, &works, ready, crate::safe_area::insets(window, cx).safe_area.bottom, cx)
            }
        };
        // Such as the vault failing to keep the hub just paired with.
        let error = self.error.clone().map(chrome::error);
        v_flex().size_full().child(header).children(error).child(body).into_any_element()
    }

    /// The repositories by name, as client-core keeps them, and in each its workspaces in the order the desktop's
    /// sidebar starts in, last active first.
    fn list(
        &self,
        records: &Snapshot,
        works: &Works,
        ready: bool,
        bottom: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let mut sections = v_flex().w_full().px(look::ROWS_INSET).pb(bottom);
        let workspaces = records.workspaces();
        for repo in records.repos() {
            let in_repo = |&w: &usize| workspaces[w].repo_id == repo.id && listed(&workspaces[w]);
            let mut shown: Vec<usize> = (0..workspaces.len()).filter(in_repo).collect();
            if shown.is_empty() {
                continue;
            }
            Sort::default().sort(&mut shown, workspaces);
            sections = sections.child(heading(repo, muted));
            for workspace in shown.into_iter().map(|w| &workspaces[w]) {
                sections = sections.child(self.row(workspace, records, works.of(workspace), ready, cx));
            }
        }
        div().id("workspaces").flex_1().min_h_0().w_full().overflow_y_scroll().child(sections).into_any_element()
    }

    /// A workspace's row, which opens its chat, and whose menu, on a long press, offers what its row's menu does on the
    /// desktop. Its name is muted until it needs a look, as in the desktop's sidebar, so the ones that do stand out.
    fn row(
        &self,
        workspace: &Workspace,
        records: &Snapshot,
        work: Work,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = look::colors(cx);
        let chats = records.sessions(&workspace.id);
        let signal = client_ui::workspace_signal(workspace, chats);
        let needs_a_look = client_core::marking::needs_a_look(workspace, chats, Session::unread);
        let id = workspace.id.clone();
        let name = div()
            .flex_1()
            .min_w_0()
            .typeset(Type::Body)
            .truncate()
            .text_color(if needs_a_look { colors.text } else { colors.text_muted })
            .when(needs_a_look, look::Typeset::emphasis)
            .child(workspace.label().to_string());
        look::list_row(ElementId::Name(format!("workspace-{}", workspace.id).into()), ROW)
            .off(!ready)
            .child(look::mark(signal, work, look::Size::Row, cx))
            .child(name)
            .opens()
            .on_click(cx.listener(move |app, _, window, cx| app.open_workspace(&id, window, cx)))
            .when(ready, |row| {
                row.right_click_menu(self.workspace_menu(workspace, records.sessions(&workspace.id), cx))
            })
            .into_any_element()
    }

    /// Opens a workspace's chat: the one last worked in, or with none open, the one the hub finds or starts. Needs a
    /// connection, since the chat reads its history from the hub.
    pub(super) fn open_workspace(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link() else { return };
        let Some(hub) = link.read(cx).ready() else { return };
        let records = hub.feed.snapshot();
        let Some(workspace) = records.workspace(id).cloned() else { return };
        let session = Session::last_worked_in(records.sessions(id).iter().filter(|s| s.is_open())).cloned();
        let chat = cx.new(|cx| ChatView::new(workspace, session, hub, window, cx));
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
