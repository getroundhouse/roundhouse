//! The bar along the top of the file tab, over a file's diff or the file itself: the file's path on a raised pill, the
//! button that copies it while the pointer is on the bar, and whatever the view puts at the bar's right end. The pill
//! has no icon for the file's kind, since `docs/spec/files.md` keeps those to All files. A changed image's bar lays out
//! its path its own way, so it starts from [`bar`] and copies with [`copy_path`].

use gpui_kit::base::ObservedElement;
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Stateful, Styled, div};
use look::{InRegion as _, Region, Shaped as _, Typeset as _};

use crate::file_tree::split;

/// The bar for the file at `path`, with `note` after its pill, such as the path it was renamed from, and `end` at its
/// right end.
pub(crate) fn file_header(path: &str, note: Option<String>, end: Option<AnyElement>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let (folders, name) = split(path);
    let (folders, name) = (folders.to_string(), name.to_string());
    // The folders give way before the name does, as they do in the sidebar. As tall as a regular button, as the switch
    // of views beside it in a diff is.
    let pill = h_flex()
        .flex_shrink(1.)
        .min_w_0()
        .h(look::ControlSize::Regular.height())
        .px(look::Space::Small)
        .corners(look::Corner::Control)
        .bg(colors.raised)
        .whitespace_nowrap()
        .when(!folders.is_empty(), |pill| {
            pill.child(
                div().flex_shrink(1.).min_w_0().overflow_hidden().text_ellipsis().text_color(muted).child(folders),
            )
        })
        .child(div().flex_shrink_0().child(name));
    bar(cx)
        .px(look::Space::Medium)
        .gap(look::Space::Small)
        .child(pill)
        .child(copy_path(path, cx))
        .children(note.map(|note| {
            div()
                .flex_shrink(1.)
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(muted)
                .child(note)
        }))
        .child(div().flex_1())
        .children(end)
        .into_any_element()
}

/// What a bar with a file's path is grouped as, so its Copy button shows while the pointer is anywhere on it.
const GROUP: &str = "file-bar";

/// A bar along the top of the file tab, with nothing in it yet: a regular bar's height, across the tab, over its edge.
pub(crate) fn bar(cx: &App) -> ObservedElement<Stateful<Div>> {
    h_flex()
        .group(GROUP)
        .flex_none()
        .w_full()
        .h(look::BarSize::Regular.height())
        .border_b_1()
        .border_color(look::colors(cx).edge)
        .typeset(look::Type::Body)
        .region(Region::FileBar)
}

/// The button that copies `path`, shown only while the pointer is on its bar, so the path reads on its own otherwise.
/// Hidden rather than left out, so what's after it doesn't move when it shows.
pub(crate) fn copy_path(path: &str, cx: &App) -> AnyElement {
    let copy = look::copy_button("copy-file-path", path.to_string(), "Copy path", cx);
    div().flex_none().invisible().group_hover(GROUP, Styled::visible).child(copy).into_any_element()
}
