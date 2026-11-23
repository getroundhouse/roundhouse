//! How the app looks.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Stateful,
    Styled, TestSupportExt as _, Window, div,
};
use look::Typeset as _;
use protocol::client::{HubStart, HubStatus};

use super::restarts::restart_note;
use super::{AppView, Connection, ConnectionFailure, corner, hub_source, hubs, pairing, title_bar, window_buttons};
use crate::menus::System;
use crate::names::{MACHINE, THIS_MACHINE};
use crate::sides::{self, AtTheTop, Side};
use crate::workspace_pane::WorkspacePane;
use crate::{CloseSettings, OpenSettings, ToggleMenus};

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Shortcuts only reach this view through whatever has focus, and focus can end up nowhere, as when what had
        // it goes away. Every shortcut would then do nothing until something is clicked, and nothing could be typed.
        if window.focused(cx).is_none() {
            self.focus_what_shows(window, cx);
        }
        self.keep_menus_current(cx);
        self.keep_dialog_current(cx);
        self.place_sidebars(window, cx);
        let arranged = crate::sidebar::arrange(self, window, cx);
        if self.settings_open() || Side::Left.shown(cx).is_none() {
            self.sidebar.not_drawn();
        }
        let home = self.home_drawn().then(|| crate::home::arrange(self, window, cx));
        if home.is_none() {
            self.home.not_drawn();
        }
        if let Some(shown) = &self.settings {
            // The workspace's keys are left out, since what they'd act on is hidden.
            let back = cx.listener(|t, _, window, cx| t.close_settings(window, cx));
            let (hub, width, chosen) = (self.settings_of_hub(), Side::Left.width(cx), shown.chosen.clone());
            return self
                .root(div(), cx)
                .child(crate::settings::page(&self.settings_focus, width, back, hub, chosen))
                .children(corner::menus(self, cx))
                .children(window_buttons::of_windows(window, cx));
        }
        let main = self.beside_the_sidebar(home, cx);
        // Built only while some of it shows.
        let sidebar = Side::Left.shown(cx).map(|shown| {
            let sidebar = crate::sidebar::render(self, arranged, cx).into_any_element();
            sides::frame(Side::Left, shown, sidebar, cx)
        });
        let root = self.with_shortcuts(h_flex(), cx);
        self.root(root, cx)
            .children(sidebar)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when_some(self.error.clone(), |d, error| d.child(error_bar(error, cx)))
                    .child(div().flex_1().min_h_0().child(main)),
            )
            .children(corner::menus(self, cx))
            .child(corner::corner(cx))
            .children(window_buttons::of_windows(window, cx))
    }
}

impl AppView {
    /// What shows beside the sidebar: the selected workspace, Home, with its `home` rows while it's drawn, or what
    /// there is to say with neither.
    fn beside_the_sidebar(&self, home: Option<crate::home::Rows>, cx: &mut Context<Self>) -> AnyElement {
        if let Some(rows) = home {
            return crate::home::page(self, rows, cx).into_any_element();
        }
        match self.connection() {
            Connection::Connecting if self.pane_to_draw().is_none() => {
                look::nothing_to_show("Connecting…").into_any_element()
            }
            Connection::Failed(ConnectionFailure::Local(reason)) => failed(reason, cx),
            Connection::Failed(failure) if self.pane_to_draw().is_none() => self.remote_failed(failure, cx),
            Connection::AcrossABreak(status) => {
                let may_restart = hub_source::may_restart(status, self.hub.build_number);
                across_a_break(status, self.hub.build_version, may_restart, cx)
            }
            Connection::Ready(_) | Connection::Connecting | Connection::Failed(_) => match self.pane_to_draw() {
                Some(pane) => pane.into_any_element(),
                None if self.records.repos().is_empty() => self.no_projects(cx),
                None => empty_state(
                    "Start a workspace",
                    "Pick a project in the sidebar and press + to give an agent a branch of its own.",
                    look::button(
                        "first-new-workspace",
                        look::ButtonKind::Main,
                        look::ControlSize::Large,
                        "New workspace",
                    )
                    .on_click(cx.listener(|t, _, window, cx| t.new_workspace_here(window, cx))),
                ),
            },
        }
    }

    fn no_projects(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(remote) = &self.hub.remote {
            let name = &remote.hub_name;
            let body = format!(
                "Open Roundhouse on {name} and add a repository there. Its projects and workspaces will show up here \
                 automatically."
            );
            return empty_state(format!("No projects on {name}"), body, switch_to_local("first-switch-local"));
        }
        let body = format!(
            "Each workspace is a copy of a project on its own branch, with its own agents. Start by adding a project \
             from a repository on this {MACHINE}."
        );
        empty_state(
            "Add a project",
            body,
            look::button("first-add-project", look::ButtonKind::Main, look::ControlSize::Large, "Add project")
                .on_click(cx.listener(|t, _, window, cx| t.add_repository(window, cx))),
        )
    }

    fn remote_failed(&self, failure: &ConnectionFailure, cx: &mut Context<Self>) -> AnyElement {
        let hub_name = self.hub.remote.as_ref().map_or("the hub", |hub| hub.hub_name.as_str());
        let (heading, body, pair_again) = remote_failure_words(hub_name, failure);
        let primary = if pair_again {
            let data_dir = self.hub.paths.data_dir.clone();
            look::button("pair-hub-again", look::ButtonKind::Main, look::ControlSize::Large, "Pair again…")
                .on_click(move |_, window, cx| pairing::open(data_dir.clone(), window, cx))
        } else {
            try_again(cx)
        };
        look::empty(look::EmptyKind::Failed, heading)
            .says(body)
            .answer(switch_to_local("failed-switch-local"))
            .answer(primary)
            .into_any_element()
    }

    /// The workspace to draw: the selected one, which stays on screen while the app connects to a hub again, such as
    /// one it's restarting to finish an update, or a remote hub temporarily out of reach. Otherwise what's typed for
    /// that moment would go nowhere. Unlike `selected_pane`, which is for acting on one, and so only gives one with a
    /// hub to act through.
    pub(in crate::app) fn pane_to_draw(&self) -> Option<Entity<WorkspacePane>> {
        let drawn = matches!(
            self.connection(),
            Connection::Ready(_) | Connection::Connecting | Connection::Failed(ConnectionFailure::Unreachable { .. })
        );
        self.selected().filter(|_| drawn).and_then(|id| self.panes.get(id).cloned())
    }

    /// The whole window, whatever it shows, and what it answers whatever it shows.
    fn root(&self, root: Div, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = look::colors(cx);
        let (background, foreground) = (colors.pane, colors.text);
        // Body text is the kit's small size, 14px, a step under the 16px its sizes are measured from, so its other
        // sizes and its spacing come out as the web's do.
        let root = root
            .id("app")
            .track_focus(&self.focus)
            .size_full()
            .bg(background)
            .text_color(foreground)
            .typeset(look::Type::Body)
            .on_action(cx.listener(|t, _: &OpenSettings, window, cx| t.open_settings(window, cx)))
            .on_action(cx.listener(|t, _: &CloseSettings, window, cx| t.close_settings(window, cx)))
            // F10 opens the menus wherever the app draws them.
            .when(System::THIS.draws_the_menus(), |root| {
                root.on_action(cx.listener(|t, _: &ToggleMenus, window, cx| t.toggle_menus(window, cx)))
            })
            .child(self.unreleased.hears());
        title_bar::stands_in(root, cx)
    }
}

/// Above the header, in the strip along the window's top, and clear of the window's buttons at both ends.
fn error_bar(error: String, cx: &mut Context<AppView>) -> impl IntoElement {
    let clear = sides::clearance(AtTheTop::AboveTheHeader, cx);
    look::banner(look::BannerKind::Error)
        .id("error-bar")
        .clear(clear.start, clear.end)
        .child(div().id("error-text").child(error).test_support())
        .answer(
            look::button("dismiss-error", look::ButtonKind::Borderless, look::ControlSize::Small, "Dismiss").on_click(
                cx.listener(|t, _, _, cx| {
                    t.error = None;
                    cx.notify();
                }),
            ),
        )
}

fn remote_failure_words(hub_name: &str, failure: &ConnectionFailure) -> (String, String, bool) {
    match failure {
        ConnectionFailure::Unreachable { hosts } => {
            let at = if hosts.is_empty() { String::new() } else { format!(" at {}", hosts.join(" or ")) };
            (
                format!("Can’t reach {hub_name}"),
                format!(
                    "Couldn’t connect{at}. Retrying automatically — check that {hub_name} is awake and on the same \
                     network or VPN."
                ),
                false,
            )
        }
        ConnectionFailure::Revoked => (
            format!("Access to {hub_name} was revoked"),
            format!(
                "This computer’s device credential is no longer accepted by {hub_name}. Create a new pairing link \
                 there to reconnect."
            ),
            true,
        ),
        ConnectionFailure::KeyChanged { host } => (
            format!("{hub_name} presented a different key"),
            format!(
                "{host} no longer matches its saved key. Check that you’re reaching {hub_name} before pairing again."
            ),
            true,
        ),
        ConnectionFailure::RemoteProtocolBreak(detail) => (
            format!("{hub_name} is on another protocol version"),
            format!("{detail}. Update Roundhouse on the older machine to reconnect."),
            false,
        ),
        ConnectionFailure::RemoteOther(detail) | ConnectionFailure::Local(detail) => {
            (format!("Couldn’t connect to {hub_name}"), detail.clone(), false)
        }
    }
}

fn switch_to_local(id: &'static str) -> look::Button {
    look::button(id, look::ButtonKind::Outlined, look::ControlSize::Large, format!("Switch to {THIS_MACHINE}"))
        .on_click(|_, window, cx| hubs::select(None, window, cx))
}

/// Tries the hub again, as the main action.
fn try_again(cx: &mut Context<AppView>) -> look::Button {
    look::button("try-hub-again", look::ButtonKind::Main, look::ControlSize::Large, "Try again")
        .on_click(cx.listener(|t, _, _, cx| t.connect(cx)))
}

fn failed(reason: &str, cx: &mut Context<AppView>) -> AnyElement {
    look::empty(look::EmptyKind::Failed, "Couldn't reach the hub")
        .says(reason.to_string())
        .answer(try_again(cx))
        .into_any_element()
}

/// `may_restart` says whether this app may restart the hub, which is only one a client started, and an older one.
fn across_a_break(status: &HubStatus, app_version: &str, may_restart: bool, cx: &mut Context<AppView>) -> AnyElement {
    let versions = format!("The hub is version {}, and this app is {app_version}.", status.hub_version);
    let next = if may_restart {
        restart_note(status)
    } else {
        let older = status.protocol_version < protocol::client::CLIENT_PROTOCOL_VERSION;
        match status.start() {
            HubStart::AsService if older => {
                "It's installed as a service, so it's updated by installing a newer release's hub on its machine."
            }
            // A hub a client started that's older is one the app may restart.
            HubStart::OnDemand | HubStart::AsService => "It's newer than this app, so the app needs updating.",
            HubStart::ByHand => {
                "It was started by hand, so it's yours to update or stop. To stop it and its agents, run \"roundhouse hub stop\"."
            }
        }
        .to_string()
    };
    let action = if may_restart {
        let pid = status.pid;
        look::button("restart-hub", look::ButtonKind::Main, look::ControlSize::Large, "Restart the hub")
            .on_click(cx.listener(move |t, _, _, cx| t.restart_hub(pid, cx)))
    } else {
        try_again(cx)
    };
    look::empty(look::EmptyKind::Failed, "This app can't talk to the running hub")
        .says(versions)
        .says(next)
        .answer(action)
        .into_any_element()
}

/// What an empty pane shows: the mark, what's missing and why, and the one thing to do about it.
fn empty_state(title: impl Into<SharedString>, body: impl Into<SharedString>, action: look::Button) -> AnyElement {
    look::empty(look::EmptyKind::Missing, title).says(body).answer(action).into_any_element()
}
