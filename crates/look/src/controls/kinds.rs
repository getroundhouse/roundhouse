use gpui_kit::Hsla;
use gpui_kit::component::button::{Button as KitButton, ButtonVariants as _};

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a button is for, which says how it's drawn: each area's one main action, an ordinary button, one that
    /// opens something, or one of the answers to what's on its own, such as a dialog. [`crate::button`] and the other
    /// ways to make a button each make one of a kind.
    ButtonKind {
        /// An area's one main action, with a light fill and dark text, such as Create PR, Send, Add project or Send to
        /// agent, so nothing competes with the one obvious next step.
        Main,
        /// An ordinary button, with no edge or fill until the pointer is on it, in the text's colour, such as the one
        /// that hides the sidebar, setup's or the update card's.
        Borderless,
        /// A borderless button whose words and icon are muted, among what it's beside, such as Home's choices of what
        /// to show, a row's own buttons or a project's `+`.
        Quiet,
        /// A borderless button whose words and icon are faint, in the heading over a list, such as the sidebar's Add
        /// project, or at the window's foot, such as Help and Settings.
        Faint,
        /// One that opens something, such as the pull request or a menu of choices, with an edge.
        Outlined,
        /// An answer that isn't the main one, to what stands on its own, such as a dialog's Not now, a plan's choices or
        /// a failed request's Retry, with an edge and a dark fill.
        Edged,
    }
}

impl ButtonKind {
    /// The kit's button, drawn as this kind but for its words' colour, which [`Self::words`] gives.
    pub(super) fn drawn(self, button: KitButton) -> KitButton {
        match self {
            ButtonKind::Main => button.primary(),
            ButtonKind::Borderless | ButtonKind::Quiet | ButtonKind::Faint => button.ghost(),
            ButtonKind::Outlined => button.outline(),
            ButtonKind::Edged => button,
        }
    }

    /// The colour of its words and icon where it isn't the kit's own for the kind.
    pub(super) fn words(self, colors: &Colors) -> Option<Hsla> {
        match self {
            ButtonKind::Quiet => Some(colors.text_muted),
            ButtonKind::Faint => Some(colors.text_faint),
            ButtonKind::Main | ButtonKind::Borderless | ButtonKind::Outlined | ButtonKind::Edged => None,
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
