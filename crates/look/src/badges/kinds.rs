use gpui_kit::Hsla;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a badge's tone is, which its colours follow. [`crate::badge`] makes one of a kind.
    BadgeKind {
        /// Something that's simply so, muted on a faint wash of the text's colour, such as `Default` beside an
        /// account.
        Plain,
        /// Something to watch, in the warning's colour on a faint fill of it, such as a login that ends soon.
        Warning,
        /// Something that went wrong, in the colour of an agent that stopped short on a faint fill of it, such as how
        /// many times in a row a request to the model failed.
        Failure,
    }
}

impl BadgeKind {
    /// Its words' colour and its fill.
    pub(super) fn colors(self, colors: &Colors) -> (Hsla, Hsla) {
        match self {
            BadgeKind::Plain => (colors.text_muted, colors.text.opacity(0.08)),
            BadgeKind::Warning => (colors.warning, colors.warning.opacity(0.14)),
            BadgeKind::Failure => (colors.stopped, colors.stopped.opacity(0.14)),
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
