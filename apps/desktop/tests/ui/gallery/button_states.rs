//! Every kind of button in every state, in one picture, as `look::ButtonKind` has them: each by its name, as a button
//! with words and an icon and as one that's only an icon, at rest, under the pointer, pressed, on, off and waiting.
//! The pointer really is over each one under the pointer, and really presses each pressed one, one at a time, so each
//! is drawn as the app draws it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{
    Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{ButtonKind, ControlSize, Type, Typeset as _};

use super::capture::Picture;
use super::headless::{self, Pointer};
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "button-states";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::ButtonKind";
/// The size every button on it is: a bar's or a card's own, the one most buttons are.
const SIZE: ControlSize = ControlSize::Regular;
const NAME_WIDTH: f32 = 90.;
/// Wide enough for a button with words and an icon, and a square, apart.
const STATE_WIDTH: f32 = 128.;
const HEADING: f32 = 32.;
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

/// Each state a button is drawn in, by what the sheet's heading calls it.
#[derive(Clone, Copy)]
enum State {
    AtRest,
    Over,
    Pressed,
    On,
    Off,
    Waiting,
}

impl State {
    const ALL: [State; 6] = [State::AtRest, State::Over, State::Pressed, State::On, State::Off, State::Waiting];

    fn heading(self) -> &'static str {
        match self {
            State::AtRest => "At rest",
            State::Over => "Under the pointer",
            State::Pressed => "Pressed",
            State::On => "On",
            State::Off => "Off",
            State::Waiting => "Waiting",
        }
    }

    /// What the pointer does to a button in this state, if it does anything.
    fn pointer(self) -> Option<Pointer> {
        match self {
            State::Over => Some(Pointer::Over),
            State::Pressed => Some(Pointer::Pressing),
            State::AtRest | State::On | State::Off | State::Waiting => None,
        }
    }

    /// The button as it's drawn in this state, but for the pointer, which the sheet moves.
    fn drawn(self, button: look::Button) -> look::Button {
        match self {
            State::AtRest | State::Over | State::Pressed => button,
            State::On => button.on(true),
            State::Off => button.off(true),
            State::Waiting => button.waiting(true),
        }
    }
}

/// The two shapes a button comes in: with words and an icon beside them, and only an icon.
const SHAPES: [&str; 2] = ["words", "square"];

/// The ID of the box around one of the buttons, which the pointer goes to the middle of, and whose pixels the sheet
/// takes from the picture where it's pointed at or pressed.
fn cell(kind: ButtonKind, state: State, shape: &str) -> ElementId {
    ElementId::Name(format!("{kind:?}-{}-{shape}", state.heading()).into())
}

fn sample(kind: ButtonKind, state: State, shape: &str) -> impl IntoElement {
    let id = format!("{kind:?}-{}-{shape}-button", state.heading());
    let button = match shape {
        "words" => look::button(id, kind, SIZE, "Add").icon(IconName::Plus),
        _ => look::icon_button(id, kind, SIZE, IconName::Close),
    };
    div().id(cell(kind, state, shape)).test_support().p(px(2.)).child(state.drawn(button))
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let words = |width: f32| div().w(px(width)).typeset(Type::Detail);
        let heading = h_flex()
            .h(px(HEADING))
            .border_b_1()
            .border_color(colors.edge)
            .text_color(colors.text_faint)
            .child(words(NAME_WIDTH))
            .children(State::ALL.map(|state| words(STATE_WIDTH).child(state.heading())));
        let row = |&kind: &ButtonKind| {
            let [name] = kind.written();
            let place = places::id(KIND, &name);
            // The design system's page shows each kind by its name as it is at rest.
            let states = State::ALL.map(|state| {
                let samples = h_flex().gap_1().children(SHAPES.map(|shape| sample(kind, state, shape)));
                let samples = match state {
                    State::AtRest => samples.id(place.clone()).test_support().into_any_element(),
                    _ => samples.into_any_element(),
                };
                div().w(px(STATE_WIDTH)).child(samples)
            });
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(words(NAME_WIDTH).text_color(colors.text_muted).child(name))
                .children(states)
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .child(heading)
            .children(ButtonKind::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind's buttons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows and states")]
    let (rows, states) = (ButtonKind::ALL.len() as f32, State::ALL.len() as f32);
    let width = px(MARGIN * 2. + NAME_WIDTH + states * STATE_WIDTH);
    let size = size(width, px(MARGIN * 2. + HEADING + rows * ROW));
    let names: Vec<String> = ButtonKind::ALL.iter().map(|kind| kind.written()[0].clone()).collect();
    let pointed: Vec<(ElementId, Pointer)> = ButtonKind::ALL
        .iter()
        .flat_map(|&kind| State::ALL.map(|state| (kind, state)))
        .filter_map(|(kind, state)| state.pointer().map(|pointer| (kind, state, pointer)))
        .flat_map(|(kind, state, pointer)| SHAPES.map(|shape| (cell(kind, state, shape), pointer)))
        .collect();
    headless::draw_pointing(&mut cx, size, None, |_| Sheet, &pointed, |window| Places::of(window, &[(KIND, &names)]))
}
