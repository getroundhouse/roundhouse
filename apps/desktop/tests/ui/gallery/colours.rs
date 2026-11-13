//! Every colour the app draws with, in one picture, by its name and in its group, as `look::Colors` has them: a
//! swatch of each on the pane, with its name and its value.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div, px, size,
};
use look::NamedColor;

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "colours";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
const COLUMNS: usize = 3;
const COLUMN: f32 = 300.;
const ROW: f32 = 28.;
const SWATCH: f32 = 20.;
const MARGIN: f32 = 16.;

/// The groups, in order, each with its colours.
fn groups(roles: &[NamedColor]) -> Vec<(&'static str, Vec<NamedColor>)> {
    let mut groups: Vec<(&'static str, Vec<NamedColor>)> = Vec::new();
    for role in roles {
        match groups.last_mut() {
            Some((group, members)) if *group == role.group => members.push(*role),
            _ => groups.push((role.group, vec![*role])),
        }
    }
    groups
}

/// The groups in columns, filled in order, each about as tall as the others.
fn columns(groups: Vec<(&'static str, Vec<NamedColor>)>) -> Vec<Vec<(&'static str, Vec<NamedColor>)>> {
    let rows = |group: &(&str, Vec<NamedColor>)| group.1.len() + 1;
    let total: usize = groups.iter().map(rows).sum();
    let mut columns: Vec<Vec<_>> = vec![Vec::new()];
    let mut filled = 0;
    for group in groups {
        let full = filled * COLUMNS >= total * columns.len();
        if full && columns.len() < COLUMNS {
            columns.push(Vec::new());
        }
        filled += rows(&group);
        columns.last_mut().unwrap().push(group);
    }
    columns
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let (edge, muted, faint) = (colors.edge, colors.text_muted, colors.text_faint);
        let swatch = |role: &NamedColor| {
            let chip = div().flex_none().size(px(SWATCH)).rounded(px(4.)).border_1().border_color(edge).bg(role.color);
            let name = div().w(px(150.)).text_xs().child(role.name);
            let value = div().text_xs().text_color(faint).child(role.hex());
            h_flex().h(px(ROW)).gap_2().child(chip).child(name).child(value)
        };
        let group = |(title, members): &(&'static str, Vec<NamedColor>)| -> Div {
            let title = div().h(px(ROW)).flex().items_end().pb_1().text_sm().text_color(muted).child(*title);
            v_flex().child(title).children(members.iter().map(swatch))
        };
        let columns = columns(groups(&colors.roles()));
        let columns = columns.iter().map(|column| v_flex().w(px(COLUMN)).children(column.iter().map(group)));
        h_flex().size_full().items_start().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(columns)
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    let roles = cx.update(|cx| look::colors(cx).roles());
    let tallest =
        columns(groups(&roles)).iter().map(|column| column.iter().map(|g| g.1.len() + 1).sum::<usize>()).max();
    #[expect(clippy::cast_precision_loss, reason = "a few dozen rows")]
    let rows = tallest.unwrap_or_default() as f32;
    #[expect(clippy::cast_precision_loss, reason = "three columns")]
    let width = px(MARGIN * 2. + COLUMNS as f32 * COLUMN);
    headless::draw(&mut cx, size(width, px(MARGIN * 2. + rows * ROW)), |_| Sheet)
}
