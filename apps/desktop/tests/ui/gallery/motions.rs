//! Every motion, in one picture, as `look::motion` has them: each by its name, as a bar a pixel long for each
//! millisecond it takes, as the design's board of motion draws it, and how long it takes, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{Corner, Shaped as _, Space};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "motions";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls what names each.
const KIND: &str = "look::motion";
/// Its samples long enough for the longest bar, with room after it.
const COLUMNS: Columns = Columns { name: 90., sample: 320., written: 50., row: 40. };

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let row = |&motion: &(&str, Duration)| {
        let [name, time] = look::motion::written(motion);
        let bar = div()
            .w(px(motion.1.as_secs_f32() * 1000.))
            .h(Space::Small.size())
            .corners(Corner::InlineControl)
            .bg(colors.current)
            .id(places::id(KIND, &name))
            .test_support();
        COLUMNS.row(name, div().w(px(COLUMNS.sample)).child(bar), time, cx)
    };
    look::motion::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each motion's bar.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = look::motion::ALL.iter().map(|&motion| look::motion::written(motion)[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
