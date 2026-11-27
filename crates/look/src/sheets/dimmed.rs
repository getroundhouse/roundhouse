//! What every sheet is drawn over: the rest of the screen, dimmed, and out of reach until the sheet's gone.

use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClickEvent, Div, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div,
};

use crate::colors::colors;

/// What a tap on what a sheet dims does, such as putting it away.
pub(super) type OnTap = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// `sheet`, along the bottom of what it's put in, which it fills, dimming the rest. What's under the sheet is out of
/// reach until it's gone, and a tap on what it dims, found by a test as `behind`, does `on_tap`.
pub(super) fn over_what_it_dims(behind: &'static str, on_tap: Option<OnTap>, sheet: impl IntoElement, cx: &App) -> Div {
    let behind = div()
        .id(behind)
        .test_support()
        .occlude()
        .flex_1()
        .w_full()
        .when_some(on_tap, |behind, on_tap| behind.on_click(move |event, window, cx| on_tap(event, window, cx)));
    v_flex().absolute().inset_0().bg(colors(cx).dim()).child(behind).child(sheet)
}
