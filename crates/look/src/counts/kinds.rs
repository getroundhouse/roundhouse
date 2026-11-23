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

named! {
    /// Whose lines added and removed they are, which their weight follows. [`crate::lines_changed`] makes them of a
    /// kind.
    LinesKind {
        /// A file's, or a change's of a few files, read beside it, in code's weight, such as a changed file's row, a
        /// call's line or a pull request's card.
        Plain,
        /// A workspace's in all, glanced at down a list of names, stronger, such as each workspace's row in the
        /// sidebar and Home.
        Workspace,
    }
}

impl LinesKind {
    /// Whether they're stronger than code is.
    pub(super) fn stronger(self) -> bool {
        match self {
            LinesKind::Plain => false,
            LinesKind::Workspace => true,
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
