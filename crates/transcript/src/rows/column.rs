//! Where the transcript's rows sit across a pane: a reading column, centred, with the composer under it reaching a
//! little past it on each side.

use gpui_kit::{Div, IntoElement, ParentElement as _, Pixels, Styled as _, div, px};

use crate::transcript::Model;

/// The reading column. Wide windows keep lines short enough to read, and the column stays centred.
const COLUMN: f32 = 848.0;
/// How far the composer reaches past the column on each side, so the transcript's text sits inside its edges.
const COMPOSER_REACH: f32 = 24.0;
/// The room the composer's row leaves between the chat's rows and the composer, or a question in its place.
pub const ABOVE_THE_COMPOSER: Pixels = px(4.);

/// What's left between the list and what's under it: the composer's room in a chat, and nothing in a subagent's tab,
/// which has no composer, since a subagent can't be prompted.
pub(crate) fn room_under(m: &Model) -> Pixels {
    if m.alone.is_some() { Pixels::ZERO } else { ABOVE_THE_COMPOSER }
}

/// What's drawn under the transcript in its column, such as a note, so it starts where the rows do in a pane of any
/// width. It's inset as the list insets each row, and then as the row is.
pub fn in_column(content: impl IntoElement) -> Div {
    div().w_full().px_3().child(column(content))
}

/// As wide as the composer under the transcript, which is centred under the column and wider than it on each side, for
/// the composer, and for the list's button back to its latest row, which sits on the composer's left edge. In a pane
/// too narrow for both, it keeps its own margin from the pane's edges, so it reaches less far past the column.
pub fn around_column(content: impl IntoElement) -> Div {
    let width = px(COLUMN + 2. * COMPOSER_REACH);
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(width).child(content))
}

pub(super) fn column(content: impl IntoElement) -> Div {
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(px(COLUMN)).child(content))
}
