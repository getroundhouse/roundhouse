//! Every state of a toggle, in one picture: on, off, and each of them greyed out, as `look::Toggle` draws them.

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
pub(crate) const NAME: &str = "toggle-states";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// Each state, by what the sheet calls it: whether it's on, and whether it's greyed out.
const STATES: [(&str, bool, bool); 4] =
    [("On", true, false), ("Off", false, false), ("On, greyed out", true, true), ("Off, greyed out", false, true)];
const STATE_WIDTH: f32 = 140.;
const HEIGHT: f32 = 80.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let state = |&(name, on, off): &(&'static str, bool, bool)| {
            v_flex()
                .w(px(STATE_WIDTH))
                .gap_3()
                .child(div().typeset(Type::Detail).text_color(colors.text_faint).child(name))
                .child(look::toggle(name, on).off(off))
        };
        h_flex().size_full().items_start().bg(colors.pane).p(px(MARGIN)).children(STATES.iter().map(state))
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "four states")]
    let states = STATES.len() as f32;
    headless::draw(&mut cx, size(px(MARGIN * 2. + states * STATE_WIDTH), px(HEIGHT)), |_| Sheet)
}
