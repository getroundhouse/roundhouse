//! The bar along the top of the file tab, over a file's diff or the file itself: the file's path on a raised pill, and
//! whatever the view puts at the bar's right end. The pill has no icon for the file's kind, since `docs/spec/files.md`
//! keeps those to All files.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div};
use look::{Shaped as _, Typeset as _};

use crate::file_tree::split;
use crate::file_view::HEADER_HEIGHT;

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
        .px_2()
        .corners(look::Corner::Control)
        .bg(colors.raised)
        .whitespace_nowrap()
        .when(!folders.is_empty(), |pill| {
            pill.child(
                div().flex_shrink(1.).min_w_0().overflow_hidden().text_ellipsis().text_color(muted).child(folders),
            )
        })
        .child(div().flex_shrink_0().child(name));
    h_flex()
        .flex_none()
        .w_full()
        .h(HEADER_HEIGHT)
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(colors.edge)
        .typeset(look::Type::Body)
        .child(pill)
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
