//! Every strength a colour is drawn at over another, in one picture, as `look::Strength` has them: each by its name, as
//! a few roles at it over the pane, with how much of each shows as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{Corner, Shaped as _, Space, Strength};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "strengths";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::Strength";
const SWATCH: f32 = 24.;
/// Its samples wide enough for a swatch of each role and the room between them.
const COLUMNS: Columns = Columns { name: 90., sample: 140., written: 50., row: 40. };

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let roles = [colors.text, colors.brand, colors.needs_you, colors.stopped, colors.added];
    let swatch = move |strength: Strength, role| div().size(px(SWATCH)).corners(Corner::Glyph).bg(strength.of(role));
    let row = |&strength: &Strength| {
        let [name, share] = strength.written();
        let place = places::id(KIND, &name);
        let swatches = roles.map(|role| swatch(strength, role));
        let sample = h_flex().w(px(COLUMNS.sample)).gap(Space::XSmall).children(swatches).id(place).test_support();
        COLUMNS.row(name, sample, share, cx)
    };
    Strength::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each strength's swatches.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = Strength::ALL.iter().map(|strength| strength.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
