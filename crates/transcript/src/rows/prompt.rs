//! What a prompt looks like: what was typed, and the files it carries.

use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled};

use super::row;
use crate::item_key::ItemKey;

/// What was sent, with the files it carries above its text, in the order the agent was given them.
pub(super) fn prompt_bubble(
    key: &ItemKey,
    text: String,
    carried: Option<AnyElement>,
    pending: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let typed = !text.trim().is_empty();
    row()
        .child(
            v_flex()
                .w_full()
                .gap_2p5()
                .px_3p5()
                .py_2p5()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .when(pending, |d| d.opacity(0.6))
                .children(carried)
                .when(typed, |d| {
                    d.child(TextView::markdown(key.element("prompt"), SharedString::from(text)).selectable(true))
                }),
        )
        .into_any_element()
}
