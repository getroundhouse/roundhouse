//! Every way a line is drawn, in one picture, as `look::line` and `look::Lined` draw them: under a bar, under each of a
//! list's rows, as Settings' are, between a card's parts, on a card washed in a colour, and across the chat with what
//! happened there in its middle.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div,
    px, size,
};
use look::{BarKind, BarSize, CardKind, CardWidth, Lined as _, Type, Typeset as _};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "lines";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 24.;
const WIDTH: f32 = 420.;
const HEIGHT: f32 = 360.;

fn muted(words: &'static str, cx: &App) -> AnyElement {
    div().text_color(look::colors(cx).text_muted).child(words).into_any_element()
}

/// A bar, and a list of rows under it, each with a line under it, as Settings' are.
fn bar_and_rows(cx: &App) -> AnyElement {
    let row =
        |words: &'static str| h_flex().h(px(48.)).px(look::Space::Large).line_under(cx).child(words).into_any_element();
    v_flex()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .bg(look::colors(cx).pane)
        .child(look::bar(BarKind::Words, BarSize::Regular).child(div().emphasis().child("Settings")))
        .children(["Always expand thinking", "Keep computer awake", "Mute sound while dictating"].map(row))
        .into_any_element()
}

/// A card with a line between its parts, a washed card with one over its last part, and a line across the chat.
fn cards_and_chat(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let card = look::card(CardKind::Over, CardWidth::Fills)
        .child(div().emphasis().child("Context"))
        .rule()
        .child(muted("42% of 200k tokens", cx));
    let washed = look::card(CardKind::Stopped, CardWidth::Fills)
        .child(muted("The request failed: overloaded", cx))
        .child(div().pt(look::Space::Small).line_over_on(CardKind::Stopped, cx).child(muted("Tried 3 times", cx)));
    let middle = div().typeset(Type::Detail).text_color(colors.text_faint).child("Compacted");
    v_flex()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .p(look::Space::Large)
        .gap(look::Space::XLarge)
        .bg(colors.pane)
        .child(card)
        .child(washed)
        .child(look::line().middle(middle))
        .into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        h_flex()
            .size_full()
            .items_start()
            .gap(px(BETWEEN))
            .p(px(MARGIN))
            .bg(colors.inset)
            .typeset(Type::Body)
            .text_color(colors.text)
            .child(bar_and_rows(cx))
            .child(cards_and_chat(cx))
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    let size = size(px(MARGIN * 2. + WIDTH * 2. + BETWEEN), px(MARGIN * 2. + HEIGHT));
    headless::draw(&mut cx, size, |_| Sheet)
}
