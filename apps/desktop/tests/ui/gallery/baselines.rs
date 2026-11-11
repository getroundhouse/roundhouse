//! Each kind of text a line can mix, set on body text's baseline, in rows of several heights, drawn at several scales.
//! Where a glyph lands depends on how GPUI rounds a line and a move to whole device pixels, which differs from one
//! scale to another, so this finds where each letter was drawn, rather than working it out again.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, black, div, px,
    size, white,
};
use look::LineText;

use super::headless;

/// Displays' usual scales, fractional ones included. None is above the app's, so all of it fits in the picture.
pub(crate) const SCALES: [f32; 5] = [1., 1.25, 1.5, 1.75, 2.];
/// So each row's middle falls on a whole device pixel at some scales and between two at others.
const ROWS: [f32; 4] = [28., 35., 36., 41.];
/// Each kind's column: body, small and code.
const CELL: f32 = 40.;
const KINDS: usize = 3;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_text = LineText::new(window, cx);
        let kinds = [&line_text.body, &line_text.small, &line_text.code];
        let cells = |row: Div| kinds.iter().fold(row, |row, kind| row.child(kind.set(div().w(px(CELL)).child("H"))));
        // Centred, as most rows are, then lined up along the top, as the outline's are.
        let centred = ROWS.map(|height| cells(h_flex().h(px(height))));
        let along_the_top = ROWS.map(|height| cells(div().flex().h(px(height))));
        v_flex().size_full().bg(black()).text_color(white()).children(centred).children(along_the_top)
    }
}

/// For each row, the lowest row of device pixels each kind's letter is drawn in, at `scale`. An "H" stands on the
/// baseline, and is flat along it.
pub(crate) fn bottoms(text_system: &Arc<dyn PlatformTextSystem>, scale: f32) -> Vec<[u32; KINDS]> {
    let mut cx = headless::app(text_system);
    let (width, height) = (CELL * 3., ROWS.iter().sum::<f32>() * 2.);
    let image = headless::draw_at(&mut cx, size(px(width), px(height)), Some(scale), |_| Sheet).image;
    let mut top = 0.;
    ROWS.iter()
        .chain(&ROWS)
        .map(|row| {
            let rows = device_pixel(top * scale)..device_pixel((top + row) * scale);
            top += row;
            std::array::from_fn(|kind| {
                #[expect(clippy::cast_precision_loss, reason = "one of three columns")]
                let left = kind as f32 * CELL;
                let columns = device_pixel(left * scale)..device_pixel((left + CELL) * scale);
                let inked = |y: &u32| columns.clone().any(|x| image.get_pixel(x, *y)[0] > 128);
                rows.clone().rev().find(inked).unwrap_or_else(|| panic!("no letter drawn at {scale}x"))
            })
        })
        .collect()
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a few hundred pixels, never negative")]
fn device_pixel(at: f32) -> u32 {
    at.round() as u32
}
