//! The lists a page of its own draws, such as Agents' and Git's: a list's heading, its rows under a line, what it says
//! in place of rows, and the badge that marks a default.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _,
    TestSupportExt as _, div,
};
use look::Lined as _;
use look::Typeset as _;

/// A list's name, with a badge after it if it has one, what it says of itself under that, and what it does at the far
/// end, if anything.
pub(super) fn heading(
    name: &'static str,
    badge: Option<look::Badge>,
    says: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> Div {
    let name = h_flex().gap(look::Space::Small).child(div().emphasis().child(name)).children(badge);
    let said = v_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::XSmall)
        .child(name)
        .child(div().typeset(look::Type::Body).text_color(look::colors(cx).text_muted).child(says.into()));
    h_flex().items_end().justify_between().gap(look::Space::XLarge).pb(look::Space::Medium).child(said).children(action)
}

/// The rows of a list, under a line, each with a line under it.
pub(super) fn list(cx: &App) -> Div {
    v_flex().line_over(cx)
}

/// What a list says in place of rows, or before them, known to tests by `id`.
pub(super) fn said_in_the_list(id: &'static str, said: SharedString, cx: &App) -> impl IntoElement + use<> {
    let colors = look::colors(cx);
    div()
        .id(id)
        .py(look::Space::Large)
        .line_under(cx)
        .typeset(look::Type::Body)
        .text_color(colors.text_muted)
        .child(said)
        .test_support()
}

/// The badge that marks what's the default, such as beside an account's name.
pub(super) fn default_badge() -> look::Badge {
    look::badge(look::BadgeKind::Plain, "Default")
}
