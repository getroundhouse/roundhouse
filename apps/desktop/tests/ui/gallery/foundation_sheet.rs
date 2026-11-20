//! The frame every sheet of a foundation shares: a row for each name, of the name, a sample drawn with it and what the
//! spec's table says of it, over a line, down the pane with room around them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, PlatformTextSystem, Render, SharedString, Styled as _,
    Window, div, px, size,
};
use look::{Lined as _, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::Places;

/// The room around a sheet's rows.
const MARGIN: f32 = 16.;

/// How a foundation's sheet is laid out: how wide each row's name, its sample and what's written of it are, and how
/// tall a row is.
pub(crate) struct Columns {
    pub(crate) name: f32,
    pub(crate) sample: f32,
    pub(crate) written: f32,
    pub(crate) row: f32,
}

/// A sheet's rows, each made with [`Columns::row`].
type Rows = fn(&App) -> Vec<AnyElement>;

struct Sheet(Rows);

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children((self.0)(cx))
    }
}

impl Columns {
    /// A row: `name`, then `sample`, which is as wide as its column and marks its own place, then `written`, what the
    /// spec's table says of it.
    pub(crate) fn row(
        &self,
        name: impl Into<SharedString>,
        sample: impl IntoElement,
        written: impl Into<SharedString>,
        cx: &App,
    ) -> AnyElement {
        let colors = look::colors(cx);
        let detail = |width: f32| div().w(px(width)).typeset(Type::Detail);
        h_flex()
            .h(px(self.row))
            .line_under(cx)
            .child(detail(self.name).text_color(colors.text_muted).child(name.into()))
            .child(sample)
            .child(detail(self.written).text_color(colors.text_faint).child(written.into()))
            .into_any_element()
    }

    /// The sheet of `rows`, at the app's scale, as the screens are, with where it draws each name of `named`, which
    /// has one row each: the names that go with the type that gives them, as [`Places::of`] takes them.
    pub(crate) fn draw(
        &self,
        text_system: &Arc<dyn PlatformTextSystem>,
        named: &[(&'static str, &[String])],
        rows: Rows,
    ) -> (Picture, Places) {
        let mut cx = headless::app(text_system);
        #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
        let count = named.iter().map(|(_, names)| names.len()).sum::<usize>() as f32;
        let width = px(MARGIN * 2. + self.name + self.sample + self.written);
        let size = size(width, px(MARGIN * 2. + count * self.row));
        headless::draw_and_measure(&mut cx, size, None, move |_| Sheet(rows), |window| Places::of(window, named))
    }
}
