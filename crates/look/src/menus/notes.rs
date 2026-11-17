use gpui_kit::Hsla;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a note at a menu's row's end says, which its colour follows. [`crate::MenuRow::note`] gives a row one.
    NoteKind {
        /// Something to know about what the row names, faint, such as a hub's address or how long ago a chat was
        /// closed.
        Detail,
        /// How many need a look, in the text's colour and stronger, such as the workspaces on another hub.
        Count,
        /// What went wrong, in the error's colour, such as a hub that can't be reached.
        Failure,
    }
}

impl NoteKind {
    /// Its colour.
    pub(super) fn color(self, colors: &Colors) -> Hsla {
        match self {
            NoteKind::Detail => colors.text_faint,
            NoteKind::Count => colors.text,
            NoteKind::Failure => colors.error,
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
