//! How the app looks.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Entity, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    Stateful, StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px,
};
use look::Hand as _;
use protocol::client::HubStatus;

use super::{AppView, Connection, corner, hub_source, title_bar, window_buttons};
use crate::menus::System;
use crate::sides::{self, AtTheTop, Side};
use crate::workspace_pane::WorkspacePane;
use crate::{CloseSettings, OpenSettings, ToggleMenus};

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Shortcuts only reach this view through whatever has focus, and focus can end up nowhere, as
        // when what had it goes away. Every shortcut would then do nothing until something is clicked.
        if window.focused(cx).is_none() {
            window.focus(if self.settings_open() { &self.settings_focus } else { &self.focus }, cx);
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
        if self.settings_open() {
            // The workspace's keys are left out, since what they'd act on is hidden.
            let back = cx.listener(|t, _, window, cx| t.close_settings(window, cx));
            let (hub, width) = (self.settings_of_hub(), Side::Left.width(cx));
            return self
                .root(div(), cx)
                .child(crate::settings::page(&self.settings_focus, width, back, hub))
                .children(corner::menus(self, cx))
                .children(window_buttons::of_windows(window, cx));
        }
        let danger = cx.theme().danger;
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
                    .when_some(self.error.clone(), |d, error| d.child(error_bar(error, danger, cx)))
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
        let (muted, danger) = (cx.theme().muted_foreground, cx.theme().danger);
        match &self.connection {
            Connection::Connecting if self.pane_to_draw().is_none() => centered("Connecting…", muted),
            Connection::Failed(reason) => failed(reason, danger, cx),
            Connection::AcrossABreak(status) => {
                let may_restart = hub_source::may_restart(status, self.hub.build_number);
                across_a_break(status, self.hub.build_version, may_restart, muted, cx)
            }
            Connection::Ready(_) | Connection::Connecting => match self.pane_to_draw() {
                Some(pane) => pane.into_any_element(),
                None if self.records.repos().is_empty() => empty_state(
                    "Add a project",
                    "Each workspace is a copy of a project on its own branch, with its own agents. Start by adding a \
                     project from a repository on this Mac.",
                    Button::new("first-add-project")
                        .hand()
                        .primary()
                        .label("Add project")
                        .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
                    cx,
                ),
                None => empty_state(
                    "Start a workspace",
                    "Pick a project in the sidebar and press + to give an agent a branch of its own.",
                    Button::new("first-new-workspace")
                        .hand()
                        .primary()
                        .label("New workspace")
                        .on_click(cx.listener(|t, _, window, cx| t.new_workspace_here(window, cx))),
                    cx,
                ),
            },
        }
    }

    /// The workspace to draw: the selected one, which stays on screen while the app connects to a hub again, such as
    /// one it's restarting to finish an update. Otherwise what's typed for that moment would go nowhere. Unlike
    /// `selected_pane`, which is for acting on one, and so only gives one with a hub to act through.
    pub(super) fn pane_to_draw(&self) -> Option<Entity<WorkspacePane>> {
        let drawn = matches!(self.connection, Connection::Ready(_) | Connection::Connecting);
        self.selected.as_ref().filter(|_| drawn).and_then(|id| self.panes.get(id).cloned())
    }

    /// The whole window, whatever it shows, and what it answers whatever it shows.
    fn root(&self, root: Div, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        // Body text is the kit's small size, 14px, a step under the 16px its sizes are measured from, so its other
        // sizes and its spacing come out as the web's do.
        let root = root
            .id("app")
            .track_focus(&self.focus)
            .size_full()
            .bg(background)
            .text_color(foreground)
            .text_sm()
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
fn error_bar(error: String, danger: Hsla, cx: &mut Context<AppView>) -> impl IntoElement {
    let clear = sides::clearance(AtTheTop::AboveTheHeader, cx);
    h_flex()
        .id("error-bar")
        .flex_none()
        .pl(px(12.) + clear.start)
        .pr(px(12.) + clear.end)
        .py_2()
        .gap_2()
        .bg(danger.opacity(0.15))
        .text_sm()
        .text_color(danger)
        .hand()
        // It can sit in the strip that moves the window, and it's a button, so a press here is its own.
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(cx.listener(|t, _, _, cx| {
            t.error = None;
            cx.notify();
        }))
        .child(div().id("error-text").flex_1().min_w_0().child(error).test_support())
        .child(div().flex_none().child("Dismiss"))
}

/// "1 chat" or "2 chats", or `None` for none.
fn counted(n: usize, what: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {what}")),
        n => Some(format!("{n} {what}s")),
    }
}

/// What `going`, such as restarting the app, would lose, or `None` for nothing: what only the app has of `chats`, such
/// as an answer being typed, or what the hub hasn't had yet, and what's typed in `comments` on the diff. It doesn't say
/// which, since there can be several kinds.
pub(super) fn unsent_note(chats: usize, comments: usize, going: &str) -> Option<String> {
    let unsent = match (counted(chats, "chat"), counted(comments, "comment")) {
        (None, None) => return None,
        (Some(both), None) | (None, Some(both)) => both,
        (Some(chats), Some(comments)) => format!("{chats} and {comments}"),
    };
    Some(format!("Something in {unsent} hasn't been sent, and {going} loses it."))
}

/// What's running on a hub, as "the 2 agents and 1 setup script", or `None` for nothing.
fn running(status: &HubStatus) -> Option<String> {
    let count = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    let agents = counted(count(status.running_sessions), "agent");
    match (agents, counted(count(status.running_setups), "setup script")) {
        (None, None) => None,
        (Some(both), None) | (None, Some(both)) => Some(format!("the {both}")),
        (Some(agents), Some(setups)) => Some(format!("the {agents} and {setups}")),
    }
}

/// That a stopped agent carries on, for after saying what stops.
fn picks_up(status: &HubStatus) -> &'static str {
    match status.running_sessions {
        0 => "",
        1 => " The agent picks up its conversation at its next prompt.",
        _ => " Each agent picks up its conversation at its next prompt.",
    }
}

/// What restarting the hub across a break would cost, since that screen can stop agents with one click.
fn restart_note(status: &HubStatus) -> String {
    let start = "Restarting it starts one that matches";
    match running(status) {
        None => format!("{start}. Nothing is running on it."),
        Some(running) => format!("{start}, and stops {running} running on it.{}", picks_up(status)),
    }
}

/// What the app asks before finishing an update by restarting the older hub. It only asks about a hub that's busy,
/// so one with nothing running is busy with something else, which restarting cuts off too.
pub(super) fn finish_question(status: &HubStatus) -> String {
    match running(status) {
        None => "Nothing is running on the previous version, but something else is using it, such as the command \
                 line. Restarting now cuts that off."
            .to_string(),
        Some(running) => format!("This stops {running} still running on the previous version.{}", picks_up(status)),
    }
}

fn try_again(cx: &mut Context<AppView>) -> Button {
    Button::new("try-hub-again").hand().label("Try again").on_click(cx.listener(|t, _, _, cx| t.connect(cx)))
}

fn failed(reason: &str, danger: Hsla, cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div().max_w(px(560.0)).text_center().text_color(danger).child(format!("Couldn't reach the hub. {reason}")),
        )
        .child(try_again(cx))
        .into_any_element()
}

/// `may_restart` says whether this app may restart the hub, which is only one a client started, and an older one.
fn across_a_break(
    status: &HubStatus,
    app_version: &str,
    may_restart: bool,
    muted: Hsla,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let versions = format!("The hub is version {}, and this app is {app_version}.", status.hub_version);
    let next = if may_restart {
        restart_note(status)
    } else if status.on_demand {
        "It's newer than this app, so the app needs updating.".to_string()
    } else {
        "It was started by hand, so it's yours to update or stop. To stop it and its agents, run \"roundhouse hub stop\"."
            .to_string()
    };
    let note = |text: String| div().max_w(px(560.0)).text_center().text_sm().text_color(muted).child(text);
    let action = if may_restart {
        let pid = status.pid;
        Button::new("restart-hub")
            .hand()
            .primary()
            .label("Restart the hub")
            .on_click(cx.listener(move |t, _, _, cx| t.restart_hub(pid, cx)))
    } else {
        try_again(cx)
    };
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child("This app can't talk to the running hub.")
        .child(note(versions))
        .child(note(next))
        .child(action)
        .into_any_element()
}

/// What an empty pane shows: the mark, what's missing and why, and the one thing to do about it.
fn empty_state(title: &str, body: &str, action: Button, cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_4()
        .child(look::brand(px(72.), cx))
        .child(
            v_flex()
                .items_center()
                .gap_1p5()
                .child(div().text_base().font_weight(gpui_kit::FontWeight::MEDIUM).child(title.to_string()))
                .child(div().max_w(px(400.)).text_center().text_sm().text_color(muted).child(body.to_string())),
        )
        .child(action)
        .into_any_element()
}

fn centered(text: &str, color: Hsla) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color)
        .child(text.to_string())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_says_how_much_it_would_lose() {
        assert_eq!(unsent_note(0, 0, "restarting"), None);
        assert_eq!(
            unsent_note(1, 0, "restarting").unwrap(),
            "Something in 1 chat hasn't been sent, and restarting loses it."
        );
        assert_eq!(
            unsent_note(0, 1, "restarting").unwrap(),
            "Something in 1 comment hasn't been sent, and restarting loses it."
        );
        assert_eq!(
            unsent_note(2, 1, "restarting").unwrap(),
            "Something in 2 chats and 1 comment hasn't been sent, and restarting loses it."
        );
    }

    /// What a hub reports with these running on it.
    fn running_on(agents: u32, setups: u32) -> HubStatus {
        HubStatus {
            pid: 1,
            hub_version: "older".to_string(),
            protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION,
            started_at_ms: 0,
            workers: agents,
            running_sessions: agents,
            clients: 1,
            on_demand: true,
            build_number: Some(1),
            running_setups: setups,
            requests: 1,
            keeping_awake: false,
            keeps_workers: false,
            oldest_protocol_version: None,
        }
    }

    #[test]
    fn the_restart_note_says_what_would_stop() {
        assert!(restart_note(&running_on(0, 0)).ends_with("Nothing is running on it."));
        assert!(restart_note(&running_on(1, 0)).contains("stops the 1 agent running on it. The agent picks up"));
        assert!(restart_note(&running_on(2, 0)).contains("stops the 2 agents running on it. Each agent picks up"));
        assert!(restart_note(&running_on(0, 2)).ends_with("stops the 2 setup scripts running on it."));
        assert!(restart_note(&running_on(2, 1)).contains("stops the 2 agents and 1 setup script running on it. Each"));
    }

    #[test]
    fn finishing_an_update_says_what_would_stop() {
        assert_eq!(
            finish_question(&running_on(0, 0)),
            "Nothing is running on the previous version, but something else is using it, such as the command line. \
             Restarting now cuts that off."
        );
        assert_eq!(
            finish_question(&running_on(2, 1)),
            "This stops the 2 agents and 1 setup script still running on the previous version. Each agent picks up \
             its conversation at its next prompt."
        );
    }
}
