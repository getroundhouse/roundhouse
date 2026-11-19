use gpui_kit::{App, Styled};

use crate::cards::CardKind;
use crate::colors::colors;
use crate::shape::EDGE;

/// Puts a line on one side of an element, in the edge's colour, such as under a bar, between two panes or under a row
/// of Settings, so no place picks its own colour or width.
pub trait Lined: Styled + Sized {
    /// A line along its bottom, between it and what's under it.
    #[must_use]
    fn line_under(self, cx: &App) -> Self {
        self.border_b(EDGE).border_color(colors(cx).edge)
    }

    /// A line along its top, between it and what's over it.
    #[must_use]
    fn line_over(self, cx: &App) -> Self {
        self.border_t(EDGE).border_color(colors(cx).edge)
    }

    /// A line down its left side, between it and what's left of it.
    #[must_use]
    fn line_left(self, cx: &App) -> Self {
        self.border_l(EDGE).border_color(colors(cx).edge)
    }

    /// A line down its right side, between it and what's right of it.
    #[must_use]
    fn line_right(self, cx: &App) -> Self {
        self.border_r(EDGE).border_color(colors(cx).edge)
    }

    /// A line along its top, inside a card of `kind`, in the colour a line on that card is, such as over a failed
    /// request's tries.
    #[must_use]
    fn line_over_on(self, kind: CardKind, cx: &App) -> Self {
        self.border_t(EDGE).border_color(kind.line(colors(cx)))
    }
}

impl<E: Styled> Lined for E {}
