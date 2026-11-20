//! Every corner the app rounds, in one picture, as `look::Corner` has them: each by its name, on a raised square with
//! the one thin edge, with how round it is as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{Corner, Shaped as _};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "corners";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::Corner";
/// Big enough that the roundest corner is plain, and the least still shows.
const SQUARE: f32 = 40.;
const COLUMNS: Columns = Columns { name: 130., sample: 90., written: 60., row: 56. };

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let row = |&corner: &Corner| {
        let [name, radius] = corner.written();
        let square = div()
            .size(px(SQUARE))
            .corners(corner)
            .border(look::EDGE)
            .border_color(colors.control_edge)
            .bg(colors.raised)
            .id(places::id(KIND, &name))
            .test_support();
        COLUMNS.row(name, div().w(px(COLUMNS.sample)).child(square), radius, cx)
    };
    Corner::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each corner's square.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = Corner::ALL.iter().map(|each| each.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
