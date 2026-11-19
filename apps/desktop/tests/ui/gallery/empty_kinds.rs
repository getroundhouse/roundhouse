//! Every kind of empty pane, in one picture, as `look::EmptyKind` has them, with the words the app says before there's
//! a project and with no hub, and a line saying there's nothing to show, as Settings' search says it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{ButtonKind, ControlSize, EmptyKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "empty-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const KIND: &str = "look::EmptyKind";
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 24.;
const PANE: (f32, f32) = (480., 380.);

/// A pane of the size the sheet draws each in, holding `inside`.
fn pane(inside: impl IntoElement, cx: &App) -> gpui_kit::Div {
    div().w(px(PANE.0)).h(px(PANE.1)).flex().bg(look::colors(cx).pane).child(inside)
}

/// A pane of `kind`, found by its kind's name.
fn empty(kind: EmptyKind, cx: &App) -> AnyElement {
    let [name, ..] = kind.written();
    let main = |words: &'static str| look::button(words, ButtonKind::Main, ControlSize::Large, words);
    let drawn = match kind {
        EmptyKind::Missing => look::empty(kind, "Add a project")
            .says(
                "Each workspace is a copy of a project on its own branch, with its own agents. Start by adding a \
                 project from a repository on this Mac.",
            )
            .answer(main("Add project")),
        EmptyKind::Failed => look::empty(kind, "Couldn't reach the hub")
            .says("starting /nonexistent/roundhoused: No such file or directory (os error 2)")
            .answer(main("Try again")),
    };
    div().id(places::id(KIND, &name)).test_support().child(pane(drawn, cx)).into_any_element()
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
            .children(EmptyKind::ALL.iter().map(|kind| empty(*kind, cx)))
            .child(pane(look::nothing_to_show("No settings match"), cx))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let kinds: Vec<String> = EmptyKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(px(MARGIN * 2. + PANE.0 * 3. + BETWEEN * 2.), px(MARGIN * 2. + PANE.1));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &kinds)]))
}
