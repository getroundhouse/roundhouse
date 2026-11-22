//! The grid as a GPUI element, which fills its parent, says how many cells fit there, and paints the frame inside one
//! layer.
use std::sync::Arc;

use client_core::drawn::{CursorShape, Frame, Rgb};
use gpui_kit::{
    App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId,
    IntoElement, LayoutId, Pixels, Style, Window, fill, relative,
};
use protocol::terminal::TerminalSize;

use super::glyphs::Glyphs;
use super::metrics::Metrics;
use super::painter::Painter;
use super::pointer::{Pointing, follow_the_pointer};
use crate::color;

/// The terminal's grid, which fills its parent.
pub(crate) struct Grid {
    pub(crate) frame: Option<Arc<Frame>>,
    pub(crate) background: Rgb,
    pub(crate) metrics: Metrics,
    pub(crate) glyphs: Glyphs,
    /// Whether the view has the keys, which a block cursor shows by being filled.
    pub(crate) focused: bool,
    /// What selected cells are painted in, under their text.
    pub(crate) selection: Hsla,
    /// Told how many cells fit, once the grid is laid out.
    pub(crate) fits: Box<dyn Fn(TerminalSize)>,
    /// Where presses and drags on the grid go.
    pub(crate) pointing: Pointing,
}

impl IntoElement for Grid {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Grid {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Hitbox {
        (self.fits)(self.metrics.fit(bounds.size));
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _: &mut App,
    ) {
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        follow_the_pointer(self.pointing.clone(), hitbox.clone(), bounds.origin, &self.metrics, window);
        // One layer for the grid, so GPUI orders it once rather than searching what each box and glyph is over. Within
        // a layer, boxes are drawn before glyphs, so a background or the cursor is always under the text.
        window.paint_layer(bounds, |window| {
            // The terminal's own, which a program can change, or swap with the text's, once there's a terminal's frame.
            let background = self.frame.as_ref().map_or(self.background, |frame| frame.background);
            window.paint_quad(fill(bounds, color::to_hsla(background)));
            let Some(frame) = self.frame.clone() else { return };
            let painter = Painter { origin: bounds.origin, metrics: &self.metrics, glyphs: &self.glyphs };
            let cursor = frame.cursor.filter(|_| self.focused);
            for (row, cells) in (0u16..).zip(&frame.rows) {
                for back in &cells.backgrounds {
                    window.paint_quad(fill(painter.cells(back.col, row, back.cells), color::to_hsla(back.color)));
                }
                if let Some(selected) = cells.selected {
                    window.paint_quad(fill(painter.cells(selected.col, row, selected.cells), self.selection));
                }
                for glyph in &cells.glyphs {
                    let under_cursor =
                        cursor.is_some_and(|at| at.shape == CursorShape::Block && at.row == row && at.col == glyph.col);
                    let ink = if under_cursor { frame.background } else { glyph.color };
                    painter.glyph(glyph, row, ink, window);
                }
            }
            if let Some(at) = frame.cursor {
                painter.cursor(at, at.color.unwrap_or(frame.foreground), self.focused, window);
            }
        });
    }
}
