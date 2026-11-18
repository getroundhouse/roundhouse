use gpui_kit::Hsla;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a count counts, which its colour and weight follow. [`crate::count`] makes one of a kind.
    CountKind {
        /// How many need a look, in the text's colour and stronger, such as a collapsed project's workspaces.
        NeedsALook,
        /// Only how many there are, muted, such as the files Changes lists.
        HowMany,
    }
}

impl CountKind {
    /// Its colour, and whether it's stronger than the text around it.
    pub(super) fn drawn(self, colors: &Colors) -> (Hsla, bool) {
        match self {
            CountKind::NeedsALook => (colors.text, true),
            CountKind::HowMany => (colors.text_muted, false),
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
