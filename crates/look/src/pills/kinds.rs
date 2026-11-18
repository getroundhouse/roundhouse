use gpui_kit::Hsla;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a pill's value is, which its colours follow. [`crate::pill`] makes one of a kind.
    PillKind {
        /// A value as it was written, muted on the raised fill, such as a command, a search's pattern or what a web
        /// search asked.
        Plain,
        /// What went wrong, as it was said, in the error's colour on a faint fill of it, such as a call's error under
        /// its line.
        Failure,
    }
}

impl PillKind {
    /// Its words' colour and its fill.
    pub(super) fn colors(self, colors: &Colors) -> (Hsla, Hsla) {
        match self {
            PillKind::Plain => (colors.text_muted, colors.raised),
            PillKind::Failure => (colors.error, colors.error.opacity(0.09)),
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
