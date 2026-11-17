//! Every kind of field in every state, in one picture, as `look::FieldKind` has them: each by its name, as a search at
//! a regular size, empty, with something typed, typed in, and off. Each typed in is pressed for real, one at a time,
//! so the cursor is in it as the app draws it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::input::InputState;
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{
    Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{ControlSize, FieldKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless::{self, Pointer};
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "field-states";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::FieldKind";
const NAME_WIDTH: f32 = 90.;
const STATE_WIDTH: f32 = 200.;
const FIELD_WIDTH: f32 = 184.;
const HEADING: f32 = 32.;
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

/// Each state a field is drawn in, by what the sheet's heading calls it.
#[derive(Clone, Copy)]
enum State {
    Empty,
    Typed,
    TypedIn,
    Off,
}

impl State {
    const ALL: [State; 4] = [State::Empty, State::Typed, State::TypedIn, State::Off];

    fn heading(self) -> &'static str {
        match self {
            State::Empty => "Empty",
            State::Typed => "With something typed",
            State::TypedIn => "Typed in",
            State::Off => "Off",
        }
    }

    /// What it says, typed in it.
    fn typed(self) -> &'static str {
        match self {
            State::Empty => "",
            State::Typed | State::TypedIn | State::Off => "fix",
        }
    }
}

/// The ID of the box around one of the fields, which a field typed in is pressed in the middle of, and whose pixels
/// the sheet takes from the picture where it's pressed.
fn cell(kind: FieldKind, state: State) -> ElementId {
    ElementId::Name(format!("{kind:?}-{}", state.heading()).into())
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Copied, since each field's input is kept with the window, which takes the context.
        let colors = look::colors(cx).clone();
        let words = |width: f32| div().w(px(width)).typeset(Type::Detail);
        let heading = h_flex()
            .h(px(HEADING))
            .border_b_1()
            .border_color(colors.edge)
            .text_color(colors.text_faint)
            .child(words(NAME_WIDTH))
            .children(State::ALL.map(|state| words(STATE_WIDTH).child(state.heading())));
        let mut row = |kind: FieldKind| {
            let [name] = kind.written();
            let place = places::id(KIND, &name);
            let states = State::ALL.map(|state| {
                let key = format!("{kind:?}-{}-input", state.heading());
                let input = window.use_keyed_state(ElementId::Name(key.into()), cx, move |window, cx| {
                    InputState::new(window, cx).placeholder("Search").default_value(state.typed())
                });
                let id = format!("{kind:?}-{}-field", state.heading());
                let field = look::field(id, &input, kind, ControlSize::Regular)
                    .icon(IconName::Search)
                    .off(matches!(state, State::Off));
                // Room around it for the ring it has while it's typed in, which is drawn outside its edge.
                let field = div().id(cell(kind, state)).test_support().p(px(4.)).w(px(FIELD_WIDTH)).child(field);
                // The design system's page shows each kind by its name as it is empty.
                let field = match state {
                    State::Empty => field.id(place.clone()).test_support().into_any_element(),
                    _ => field.into_any_element(),
                };
                div().w(px(STATE_WIDTH)).child(field)
            });
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(words(NAME_WIDTH).text_color(colors.text_muted).child(name))
                .children(states)
        };
        let rows: Vec<_> = FieldKind::ALL.iter().map(|&kind| row(kind)).collect();
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).child(heading).children(rows)
    }
}

/// At the app's scale, as the screens are, with where it draws each kind's field.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows and states")]
    let (rows, states) = (FieldKind::ALL.len() as f32, State::ALL.len() as f32);
    let width = px(MARGIN * 2. + NAME_WIDTH + states * STATE_WIDTH);
    let size = size(width, px(MARGIN * 2. + HEADING + rows * ROW));
    let names: Vec<String> = FieldKind::ALL.iter().map(|kind| kind.written()[0].clone()).collect();
    let pressed: Vec<(ElementId, Pointer)> =
        FieldKind::ALL.iter().map(|&kind| (cell(kind, State::TypedIn), Pointer::Pressing)).collect();
    headless::draw_pointing(&mut cx, size, None, |_| Sheet, &pressed, |window| Places::of(window, &[(KIND, &names)]))
}
