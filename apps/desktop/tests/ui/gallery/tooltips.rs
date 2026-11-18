//! Every tooltip, in one picture: a few words, words with the key that does the same, Escape among them, and words
//! that run past its widest and wrap, as the app draws each, where it's put rather than under the pointer.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div, px, size,
};
use look::{Type, Typeset as _};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "tooltips";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const NAME_WIDTH: f32 = 140.;
const ROW: f32 = 64.;
const MARGIN: f32 = 16.;

/// Each tooltip: what it shows, its words, and the key that does the same, if there's one.
const TOOLTIPS: [(&str, &str, Option<&str>); 4] = [
    ("Words", "Hide sidebar", None),
    ("With its key", "Settings", Some("cmd-,")),
    ("Escape", "Cancel dictation", Some("escape")),
    (
        "Words that wrap",
        "Pairs this computer with the hub on the other machine, which can then open and work in its workspaces.",
        None,
    ),
];

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (muted, pane, text) = {
            let colors = look::colors(cx);
            (colors.text_muted, colors.pane, colors.text)
        };
        let row = |&(what, words, key): &(&'static str, &'static str, Option<&'static str>),
                   window: &mut Window,
                   cx: &mut Context<Self>| {
            h_flex()
                .min_h(px(ROW))
                .items_center()
                .child(div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(muted).child(what))
                // The kit leaves room round a tooltip for its shadow, which this takes back.
                .child(h_flex().ml(px(-12.)).child(look::shown_tooltip(words, key, window, cx)))
        };
        let rows: Vec<_> = TOOLTIPS.iter().map(|tooltip| row(tooltip, window, cx)).collect();
        v_flex().size_full().bg(pane).text_color(text).p(px(MARGIN)).children(rows)
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = TOOLTIPS.len() as f32;
    headless::draw(&mut cx, size(px(MARGIN * 2. + NAME_WIDTH + 360.), px(MARGIN * 2. + rows * ROW + 24.)), |_| Sheet)
}
