//! The hub's workspaces, grouped by repository, each with its status, and opening one's chat.

use std::collections::HashMap;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use look::{Size, Work};
use protocol::model::{Repo, Session, Workspace, WorkspaceId, WorkspaceStatus};

use super::{MobileApp, Screen};
use crate::chat::ChatView;
use crate::chrome::{self, TAP_HEIGHT};
use crate::hub_link::LinkState;

/// What the list shows, read from client-core in one go.
struct Listing {
    repos: Vec<Repo>,
    workspaces: Vec<Workspace>,
    sessions: HashMap<WorkspaceId, Vec<Session>>,
}

impl MobileApp {
    pub(super) fn workspaces_screen(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        let Some(link) = self.link() else { return div().into_any_element() };
        let link = link.read(cx);
        let (name, ready) = (link.name().clone(), link.client().is_some());
        let subtitle: SharedString = match link.state() {
            LinkState::Connecting => "Connecting…".into(),
            LinkState::Ready => "Connected".into(),
            LinkState::Unreachable(_) => "Can't reach it. Trying again".into(),
        };
        let listing = link.read(|records| Listing {
            repos: records.repos(),
            workspaces: records.workspaces().into_iter().filter(|w| w.status != WorkspaceStatus::Archived).collect(),
            sessions: records.sessions(),
        });
        let hub_button = icon_button("hub-info", IconName::Settings)
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::HubInfo, cx)));
        let new_button = icon_button("new-workspace", IconName::Plus)
            .disabled(!ready)
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::NewWorkspace, cx)));
        let header = chrome::header(
            name.clone(),
            Some(subtitle),
            Some(hub_button.into_any_element()),
            Some(new_button.into_any_element()),
            insets.safe_area.top,
            cx,
        );
        let body = match listing {
            None => chrome::empty(format!("Connecting to {name}"), "", cx).into_any_element(),
            Some(listing) if listing.workspaces.is_empty() => {
                chrome::empty("No workspaces", "Tap + to start one.", cx).into_any_element()
            }
            Some(listing) => Self::list(&listing, ready, insets.safe_area.bottom, cx),
        };
        v_flex().size_full().child(header).child(body).into_any_element()
    }

    fn list(listing: &Listing, ready: bool, bottom: gpui_kit::Pixels, cx: &mut Context<Self>) -> AnyElement {
        let mut repos: Vec<&Repo> = listing.repos.iter().collect();
        repos.sort_by(|a, b| a.name.cmp(&b.name));
        let mut sections = v_flex().w_full().pb(bottom);
        for repo in repos {
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
        let signal = chrome::workspace_signal(workspace, sessions);
        let content = h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .child(look::mark(signal, Work::NoPr, Size::Row, cx))
            .child(v_flex().flex_1().min_w_0().child(div().truncate().child(workspace.name.clone())).when_some(
                chrome::branch_under_name(workspace),
                |this, branch| {
                    this.child(div().text_xs().truncate().text_color(cx.theme().muted_foreground).child(branch))
                },
            ))
            .child(div().text_color(cx.theme().muted_foreground).child(IconName::ChevronRight));
        let button = Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
            .ghost()
            .w_full()
            .h(px(60.))
            .px_4()
            .child(content)
            .on_click(cx.listener(move |app, _, window, cx| {
                if ready {
                    app.open_workspace(&id, window, cx);
                }
            }));
        div().border_b_1().border_color(cx.theme().border).child(button).into_any_element()
    }

    /// Shows a workspace's chat. Needs a connection, since the chat reads its history from the hub.
    pub(super) fn open_workspace(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link() else { return };
        let link = link.read(cx);
        let Some(client) = link.client() else { return };
        let found = link.read(|records| {
            let workspace = records.workspaces().into_iter().find(|w| &w.id == id)?;
            let sessions = records.sessions().remove(id).unwrap_or_default();
            Some((workspace, sessions))
        });
        let Some((workspace, sessions)) = found.flatten() else { return };
        let chat = cx.new(|cx| ChatView::new(workspace, &sessions, client, window, cx));
        self.show_chat(chat, cx);
    }
}

fn icon_button(id: &'static str, icon: IconName) -> Button {
    Button::new(id).icon(icon).ghost().w(px(TAP_HEIGHT)).h(px(TAP_HEIGHT))
}
