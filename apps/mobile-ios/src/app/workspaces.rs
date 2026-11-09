//! The hub's workspaces, grouped by repository, each with its status, and opening one's chat.

use std::collections::HashMap;

use client_core::records::{latest_open_session, workspace_status};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, StatefulInteractiveElement, Styled, Window, div,
};
use look::{Signal, Size, Work};
use protocol::model::{Repo, Session, Workspace, WorkspaceId, WorkspaceStatus};

use super::{Followed, MobileApp, Screen};
use crate::chat::ChatView;
use crate::chrome;

/// What the list shows, read from client-core in one go.
struct Listing {
    repos: Vec<Repo>,
    workspaces: Vec<Workspace>,
    sessions: HashMap<WorkspaceId, Vec<Session>>,
}

impl MobileApp {
    pub(super) fn workspaces_screen(&self, followed: &Followed, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let link = followed.link.read(cx);
        let (name, ready) = (link.hub().hub_name.clone(), link.client().is_some());
        // Ready ones only, as on the desktop: one still being made has nothing to open yet.
        let listing = link.read(|records| Listing {
            repos: records.repos(),
            workspaces: records.workspaces().into_iter().filter(|w| w.status == WorkspaceStatus::Ready).collect(),
            sessions: records.sessions(),
        });
        let hub_button = chrome::icon_button("hub-info", IconName::Settings)
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::HubInfo, cx)));
        let new_button = chrome::icon_button("new-workspace", IconName::Plus)
            .disabled(!ready)
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::NewWorkspace, cx)));
        let header = chrome::header(
            name.clone().into(),
            Some(chrome::connection(link.state())),
            Some(hub_button.into_any_element()),
            Some(new_button.into_any_element()),
            window,
            cx,
        );
        let body = match listing {
            None => chrome::empty(format!("Connecting to {name}"), "", cx),
            Some(listing) if listing.workspaces.is_empty() => chrome::empty("No workspaces", "Tap + to start one.", cx),
            Some(listing) => Self::list(&listing, ready, window.insets().safe_area.bottom, cx),
        };
        // Such as the vault failing to keep the hub just paired with.
        let error = self.error.clone().map(|error| chrome::error_line(error, cx));
        v_flex().size_full().child(header).when_some(error, ParentElement::child).child(body).into_any_element()
    }

    /// Newest first in each repository, and the repositories by name, as client-core keeps them.
    fn list(listing: &Listing, ready: bool, bottom: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let mut sections = v_flex().w_full().pb(bottom);
        for repo in &listing.repos {
            let mut workspaces: Vec<&Workspace> = listing.workspaces.iter().filter(|w| w.repo_id == repo.id).collect();
            if workspaces.is_empty() {
                continue;
            }
            workspaces.sort_by_key(|w| std::cmp::Reverse(w.created_at_ms));
            let title = div()
                .px_4()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().muted_foreground)
                .child(repo.name.clone());
            sections = sections.child(title);
            for workspace in workspaces {
                let sessions = listing.sessions.get(&workspace.id).map_or(&[][..], Vec::as_slice);
                sections = sections.child(Self::row(workspace, sessions, ready, cx));
            }
        }
        div().id("workspaces").flex_1().min_h_0().w_full().overflow_y_scroll().child(sections).into_any_element()
    }

    fn row(workspace: &Workspace, sessions: &[Session], ready: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = workspace.id.clone();
        let signal = Signal::for_workspace(workspace, workspace_status(sessions));
        let muted = cx.theme().muted_foreground;
        let content = h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .child(look::mark(signal, Work::NoPr, Size::Row, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(workspace.name.clone()))
                    .when_some(chrome::branch_under_name(workspace), |this, branch| {
                        this.child(div().text_xs().truncate().text_color(muted).child(branch))
                    }),
            )
            .child(div().text_color(muted).child(IconName::ChevronRight));
        let open = cx.listener(move |app, _, window, cx| {
            if ready {
                app.open_workspace(&id, window, cx);
            }
        });
        chrome::row(ElementId::Name(format!("workspace-{}", workspace.id).into()), content, open, cx)
    }

    /// Shows a workspace's chat. Needs a connection, since the chat reads its history from the hub.
    fn open_workspace(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link() else { return };
        let link = link.read(cx);
        let Some(client) = link.client() else { return };
        let found = link.read(|records| {
            let workspace = records.workspaces().into_iter().find(|w| &w.id == id)?;
            let sessions = records.sessions().remove(id).unwrap_or_default();
            Some((workspace, sessions))
        });
        let Some((workspace, sessions)) = found.flatten() else { return };
        let session = latest_open_session(&sessions).cloned();
        let chat = cx.new(|cx| ChatView::new(workspace, session, client, window, cx));
        self.show_chat(chat, cx);
    }
}
