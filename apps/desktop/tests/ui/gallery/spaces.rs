//! Every step of space, in one picture, as `look::Space` has it: each by its name, as two raised blocks with the step
//! between them, so the room shows, and how much room it is, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{Corner, Shaped as _, Space};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "spaces";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each step.
const KIND: &str = "look::Space";
const BLOCK: f32 = 32.;
/// Its samples wide enough for two blocks and the largest step between them.
const COLUMNS: Columns = Columns { name: 90., sample: 140., written: 50., row: 48. };

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let block = || div().size(px(BLOCK)).corners(Corner::Glyph).bg(colors.raised);
    let row = |&space: &Space| {
        let [name, room] = space.written();
        let place = places::id(KIND, &name);
        let sample = h_flex().w(px(COLUMNS.sample)).gap(space).child(block()).child(block()).id(place).test_support();
        COLUMNS.row(name, sample, room, cx)
    };
    Space::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each step's blocks.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = Space::ALL.iter().map(|space| space.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
