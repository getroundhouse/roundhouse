//! What the screens share: the bar across the top and its buttons, a row to tap, the line saying how the connection
//! stands, an error, and an empty screen.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement, SharedString, Styled, Window, div,
    px,
};
use protocol::model::Workspace;

use crate::hub_link::LinkState;

/// Tall enough for a thumb, which is what iOS asks of anything tapped.
pub(crate) const TAP_HEIGHT: f32 = 44.;
/// A row of a list: a name, with a line under it.
const ROW_HEIGHT: f32 = 60.;

/// The bar across the top, below the status bar: a button on each side, and a title with a line under it.
pub(crate) fn header(
    title: SharedString,
    subtitle: Option<SharedString>,
    left: Option<AnyElement>,
    right: Option<AnyElement>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let side = |button: Option<AnyElement>| div().w(px(TAP_HEIGHT)).flex_shrink_0().children(button);
    h_flex()
        .w_full()
        .flex_shrink_0()
        .pt(window.insets().safe_area.top)
        .px_2()
        .pb_1()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(side(left))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .child(div().font_weight(FontWeight::SEMIBOLD).truncate().child(title))
                .when_some(subtitle, |this, subtitle| {
                    this.child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(subtitle))
                }),
        )
        .child(side(right))
        .into_any_element()
}

pub(crate) fn icon_button(id: impl Into<ElementId>, icon: IconName) -> Button {
    Button::new(id).icon(icon).ghost().w(px(TAP_HEIGHT)).h(px(TAP_HEIGHT))
}

pub(crate) fn back_button() -> Button {
    icon_button("back", IconName::ChevronLeft)
}

/// A row the width of the screen, with a line under it.
pub(crate) fn row(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let button = Button::new(id).ghost().w_full().h(px(ROW_HEIGHT)).px_4().child(content).on_click(on_click);
    div().border_b_1().border_color(cx.theme().border).child(button).into_any_element()
}

/// How the connection stands, in a few words.
pub(crate) fn connection(state: &LinkState) -> SharedString {
    match state {
        LinkState::Connecting => "Connecting…".into(),
        LinkState::Ready => "Connected".into(),
        LinkState::Unreachable(_) => "Can't reach it. Trying again".into(),
    }
}

pub(crate) fn error_line(error: SharedString, cx: &App) -> AnyElement {
    div().px_4().py_2().text_sm().text_color(cx.theme().danger).child(error).into_any_element()
}

/// A line of text in the middle of an empty screen, saying what's missing and what to do.
pub(crate) fn empty(title: impl Into<SharedString>, detail: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .px_8()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .child(div().text_sm().text_center().text_color(cx.theme().muted_foreground).child(detail.into()))
        .into_any_element()
}

/// What to show under a workspace's name: its branch, unless that only says the name again, as the hub's own
/// choice of both does.
pub(crate) fn branch_under_name(workspace: &Workspace) -> Option<SharedString> {
    (workspace.branch != workspace.name).then(|| workspace.branch.clone().into())
}
