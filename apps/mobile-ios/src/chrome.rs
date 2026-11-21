//! What the screens share: the bar across the top, with a button back at its start and the screen's own at its end,
//! what's said when there's nothing to show or something went wrong, and how the connection stands. Each is drawn
//! from the design system's parts, at a handheld's sizes.

use gpui_kit::assets::IconName;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div};
use look::{ButtonKind, ControlSize};
use protocol::model::Workspace;

use crate::hub_link::LinkState;

/// What's tapped in the bar: large, which is as tall as iOS asks of anything tapped on a handheld.
pub(crate) const TAPPED: ControlSize = ControlSize::Large;

/// The bar across the top, clear of the status bar: maybe a button back, the screen's title with a line under it,
/// and the screen's own buttons at its end.
pub(crate) fn header(
    title: SharedString,
    under: Option<SharedString>,
    back: Option<look::Button>,
    end: Vec<look::Button>,
    window: &Window,
    cx: &App,
) -> look::TopBar {
    let bar = look::top_bar(title).clear(crate::safe_area::insets(window, cx).safe_area.top);
    let bar = under.into_iter().fold(bar, look::TopBar::under);
    let bar = back.into_iter().fold(bar, look::TopBar::start);
    end.into_iter().fold(bar, look::TopBar::end)
}

/// The button back to the list of workspaces.
pub(crate) fn back() -> look::Button {
    icon_button("back", IconName::ChevronLeft, "Back")
}

/// A button in the bar that's only an icon, which `label` names for an assistive app.
pub(crate) fn icon_button(id: impl Into<ElementId>, icon: IconName, label: &'static str) -> look::Button {
    look::icon_button(id, ButtonKind::Borderless, TAPPED, icon).accessibility_label(label)
}

/// How the connection stands, in a few words.
pub(crate) fn connection(state: &LinkState) -> SharedString {
    match state {
        LinkState::Connecting => "Connecting…".into(),
        LinkState::Ready => "Connected".into(),
        LinkState::Unreachable(_) => "Can't reach it. Trying again".into(),
        LinkState::Refused(_) => "Can't connect".into(),
    }
}

/// What went wrong, across the screen under its bar.
pub(crate) fn error(error: SharedString) -> AnyElement {
    look::banner(look::BannerKind::Error).id("error").child(error).into_any_element()
}

/// A screen with nothing to show: what's missing, and what to do.
pub(crate) fn empty(heading: impl Into<SharedString>, says: impl Into<SharedString>) -> AnyElement {
    div().flex_1().child(look::empty(look::EmptyKind::Missing, heading).says(says)).into_any_element()
}

/// What to show under a workspace's name: its branch, unless that only says the name again, as the hub's own choice
/// of both does.
pub(crate) fn branch_under_name(workspace: &Workspace) -> Option<SharedString> {
    let branch = workspace.branch.to_string();
    (branch != workspace.label()).then(|| branch.into())
}
