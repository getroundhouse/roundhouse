use gpui_kit::{Hsla, Pixels, px};

use crate::colors::Colors;
use crate::named::named;

named! {
    /// Why a pane is empty, which says the colour of the mark over what it says. [`crate::empty`] makes one of a kind.
    EmptyKind {
        /// Nothing's there yet, such as before there's a project or a workspace: the mark in copper, the brand, over
        /// what's missing and the one thing to do about it.
        Missing,
        /// What the pane is for can't be shown, such as with no hub, or one that can't be reached or talked to: the
        /// mark in red, over what went wrong, why, and what to do about it.
        Failed,
    }
}

/// How wide the mark over an empty pane is, with five rows of dots.
pub(super) const MARK: Pixels = px(72.);
/// How wide what an empty pane says under its heading may run before it wraps.
pub(super) const SAYS: Pixels = px(400.);

impl EmptyKind {
    /// The colour of its mark.
    pub(super) fn mark(self, colors: &Colors) -> Hsla {
        match self {
            EmptyKind::Missing => colors.brand,
            EmptyKind::Failed => colors.error,
        }
    }

    /// How the spec's table writes it: its name, and the mark's colour by its name in `look.md`.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        let mark = match self {
            EmptyKind::Missing => "brand",
            EmptyKind::Failed => "error",
        };
        [format!("{self:?}"), mark.to_string()]
    }
}
