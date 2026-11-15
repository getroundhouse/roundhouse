use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How big an icon is, as wide as it's tall. One set among text, as one of its words, is about as big as the text,
    /// and one that stands for what a row, a tab, a bar, a card or a call's line is, at its start, is as big as a mark
    /// beside text. [`IconSizing::sized`] sizes an icon as one. An icon on a button is sized by the button's
    /// [`ControlSize`](crate::ControlSize), which says which of these it is, and one inside a control about as tall as
    /// a line of text, such as the check in a checkbox, is small, so it has room inside it.
    IconSize {
        /// Among a detail's or code's text: a rule's across the chat, such as a compaction's, and the compaction's in
        /// the outline; the build line's answer; a changed file's state at its row's end, and a folder's chevron, in
        /// Changes; the check in a checkbox; and the `+` that adds a comment to a diff's line, on a button as tall as
        /// the line. Beside a small button's words.
        Small,
        /// Among body text: a warning before the body text it warns of, such as an account's standing; a project's
        /// chevron after its name; the arrow before an answer to a question, and the check on an option chosen; a file
        /// sent with a prompt waiting in the queue; a draft's pencil at a workspace's row's end; the arrow that reopens
        /// a closed chat, in its menu; the glass in a search field; the arrow after a pull request's number; and the
        /// update card's details. Beside a regular button's words, and a small or regular button's own, as a square,
        /// such as Send, a tab's close or closed chats.
        Regular,
        /// Standing for what a row, a tab, a bar, a card or a call's line is, at its start, in the place a workspace or
        /// a chat has its mark: a file's or a folder's, in All files or Changes; a place's or a project's in the
        /// sidebar; a page's in Settings; an account's; a file tab's, and a subagent's, on its tab and its bar; a
        /// file's card, sent with a prompt; a failed request's; and a call's kind, or the chevron that takes its place,
        /// and the robot of a subagent at work. Beside a large button's words, and a large button's own.
        Large,
    }
}

impl IconSize {
    /// How wide and tall it is.
    pub const fn size(self) -> Pixels {
        match self {
            IconSize::Small => px(12.),
            IconSize::Regular => px(14.),
            IconSize::Large => px(16.),
        }
    }

    /// How the spec's table writes it: its name and how big it is.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.size()))]
    }
}

/// Sizes an icon as one of the sizes an icon comes in, rather than as one of the kit's or as the text around it.
pub trait IconSizing {
    /// Sizes it as `size`. A button that's given it as its icon sizes it again, by its own size.
    #[must_use]
    fn sized(self, size: IconSize) -> Self;
}

impl IconSizing for Icon {
    fn sized(self, size: IconSize) -> Self {
        self.with_size(size.size())
    }
}
