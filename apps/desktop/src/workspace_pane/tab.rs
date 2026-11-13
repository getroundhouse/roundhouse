//! What every tab in the row is built from, whatever it shows, so the row reads as one row of tabs: the same width,
//! padding and underline, and a close button that shows only while the pointer is over the tab.

use gpui_kit::base::Tab;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, ElementId, InteractiveElement as _, ParentElement as _, Pixels, SharedString, Stateful, Styled,
    Window, div, px, relative,
};
use look::Hand as _;

use super::WorkspacePane;

/// Every tab is the same width, so the row doesn't shift when a chat gets its title.
const TAB_WIDTH: Pixels = px(200.);
/// Each tab's, so a tab's close button shows only while the pointer is over that tab.
const TAB_GROUP: &str = "chat-tab";

/// The part of a tab that's clicked to show what it's for, named `label` for whoever can't see it. What it shows goes
/// in it, and [`tab`] makes the tab of it.
pub(super) fn face(id: impl Into<ElementId>, selected: bool, label: impl Into<SharedString>) -> Tab {
    Tab::new(id)
        .hand()
        .selected(selected)
        .accessibility_label(label)
        .size_full()
        .justify_start()
        .gap_2()
        .pl_3()
        .pr_1p5()
        .text_sm()
        .line_height(relative(1.25))
}

/// Where a tab's name goes, cut short where it doesn't fit.
pub(super) fn name() -> Div {
    div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis()
}

/// A tab's close button, which closes it without the tab under it taking the click too, and showing what's closing.
pub(super) fn close_button(
    id: impl Into<ElementId>,
    tooltip: &'static str,
    cx: &mut Context<WorkspacePane>,
    close: impl Fn(&mut WorkspacePane, &mut Window, &mut Context<WorkspacePane>) + 'static,
) -> Button {
    Button::new(id).hand().ghost().xsmall().icon(IconName::Close).tooltip(tooltip).on_click(cx.listener(
        move |t, _, window, cx| {
            cx.stop_propagation();
            close(t, window, cx);
        },
    ))
}

/// A tab of `face`, with its close button at the end, and underlined while it's showing.
pub(super) fn tab(id: impl Into<ElementId>, face: Tab, close: Button, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .group(TAB_GROUP)
        .relative()
        .flex_none()
        .w(TAB_WIDTH)
        .h_full()
        // Hidden rather than left out, so the name doesn't move when it shows.
        .child(face.child(div().flex_none().invisible().group_hover(TAB_GROUP, Styled::visible).child(close)))
        .when(selected, |d| d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline())))
}
