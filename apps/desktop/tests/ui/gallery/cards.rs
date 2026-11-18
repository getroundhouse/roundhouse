//! Every kind of card and every width, in one picture, as `look::CardKind` and `look::CardWidth` have them: each kind
//! with what a card of it holds in the app, on the pane, then a card of each width.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{ButtonKind, CardKind, CardWidth, ControlSize, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "card-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const KIND: &str = "look::CardKind";
const WIDTH: &str = "look::CardWidth";
/// How wide the column a card that fills is set in is.
const COLUMN: f32 = 520.;
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 48.;

fn muted(words: &'static str, cx: &App) -> AnyElement {
    div().text_color(look::colors(cx).text_muted).child(words).into_any_element()
}

fn detail(words: &'static str, cx: &App) -> AnyElement {
    div().typeset(Type::Detail).text_color(look::colors(cx).text_muted).child(words).into_any_element()
}

fn answer(id: &'static str, kind: ButtonKind, size: ControlSize, words: &'static str) -> look::Button {
    look::button(id, kind, size, words)
}

/// A card of `kind`, as the app draws one, filling what it's set in unless it opens over the rest.
fn of_kind(kind: CardKind, cx: &App) -> look::Card {
    match kind {
        CardKind::Over => look::card(kind, CardWidth::Small)
            .heading(look::card_heading("plum"))
            .under_heading(detail("Built from ~/dev/roundhouse/plum", cx))
            .under_heading(detail("Its hub keeps its data in .context/dev", cx)),
        CardKind::InPlace => look::card(kind, CardWidth::Fills)
            .heading(look::card_heading("Roundhouse 1.0.0 is ready"))
            .under_heading(muted("Restart to use it. What you haven't sent stays where it is.", cx))
            .child(h_flex().child(answer("restart", ButtonKind::Outlined, ControlSize::Regular, "Restart"))),
        CardKind::Chat => look::card(kind, CardWidth::Fills)
            .under_heading(
                div()
                    .typeset(Type::Detail)
                    .text_color(look::colors(cx).text_faint)
                    .child("What the agent carries on from"),
            )
            .under_heading(muted("The sidebar reads each workspace's pull request from the hub's notice.", cx)),
        CardKind::NeedsYou => look::card(kind, CardWidth::Fills)
            .heading(look::card_heading("Plan").mark(IconName::NotebookPen).end("Waiting for you"))
            .child(div().child("Show each workspace's pull request in the sidebar, from the hub."))
            .child(
                h_flex()
                    .gap(look::Space::Small)
                    .child(answer("approve", ButtonKind::Main, ControlSize::Large, "Approve plan"))
                    .child(answer("keep-planning", ButtonKind::Edged, ControlSize::Large, "Keep planning")),
            ),
        CardKind::Stopped => look::card(kind, CardWidth::Fills)
            .heading(
                look::card_heading("Usage limit reached")
                    .mark(IconName::CircleX)
                    .after(look::badge(look::BadgeKind::Failure, "Twice"))
                    .end("Resets 5:00 PM"),
            )
            .child(muted("You've reached your usage limit. Switch to another model to continue.", cx))
            .child(h_flex().child(answer("retry", ButtonKind::Edged, ControlSize::Large, "Retry"))),
    }
}

/// A card of `width` over the rest, saying what it's for.
fn of_width(width: CardWidth, cx: &App) -> look::Card {
    let said = match width {
        CardWidth::Small => "A few lines, such as a hover card, the context's card or a turn's.",
        CardWidth::Regular => "More, or what runs across a line of its own, such as the workspace's card.",
        CardWidth::Fills => "As wide as what it's set in, such as the chat's column.",
    };
    let [name, _] = width.written();
    look::card(CardKind::Over, width).heading(look::card_heading(name)).under_heading(detail(said, cx))
}

/// `sample`, under its name, faint, with its place found by `kind` and `name`.
fn named(kind: &str, name: &str, sample: impl IntoElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    v_flex()
        .gap(look::Space::Small)
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(name.to_string()))
        .child(div().id(places::id(kind, name)).test_support().child(sample))
        .into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let kind = |kind: CardKind, cx: &App| named(KIND, &kind.written()[0], of_kind(kind, cx), cx);
        let width = |width: CardWidth, cx: &App| named(WIDTH, &width.written()[0], of_width(width, cx), cx);
        let column = |width: f32| v_flex().flex_none().w(px(width)).gap(px(BETWEEN / 2.));
        h_flex()
            .size_full()
            .items_start()
            .gap(px(BETWEEN))
            .p(px(MARGIN))
            .bg(colors.pane)
            .text_color(colors.text)
            .child(column(300.).child(kind(CardKind::Over, cx)).child(kind(CardKind::InPlace, cx)))
            .child(
                column(COLUMN)
                    .child(kind(CardKind::Chat, cx))
                    .child(kind(CardKind::NeedsYou, cx))
                    .child(kind(CardKind::Stopped, cx)),
            )
            .child(
                column(COLUMN)
                    .child(width(CardWidth::Small, cx))
                    .child(width(CardWidth::Regular, cx))
                    .child(width(CardWidth::Fills, cx)),
            )
    }
}

/// At the app's scale, as the screens are, with where it draws each kind and each width.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let kinds: Vec<String> = CardKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let widths: Vec<String> = CardWidth::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(px(MARGIN * 2. + 300. + COLUMN * 2. + BETWEEN * 2.), px(460.));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Sheet,
        |window| Places::of(window, &[(KIND, &kinds), (WIDTH, &widths)]),
    )
}
