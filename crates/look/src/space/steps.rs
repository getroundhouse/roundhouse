use gpui_kit::{DefiniteLength, Length, Pixels, px};

use crate::named::named;

named! {
    /// A step of the room left between and around things, as a gap, a padding or a margin, to set them apart or hold
    /// them together. Every space is one, on a 4px grid. They're sizes, ordered from the smallest, and what belongs
    /// together takes a smaller step than what's apart. A length set for something else, such as one that lines a thing
    /// up with the text beside it, isn't a space, and takes none.
    Space {
        /// Between a name and what's said under it, such as an account's; between the lines of one list, such as a
        /// project's row and its workspaces or Settings' pages; between the buttons on a row, such as a prompt's
        /// waiting in the queue; and around the rows in All files, in Changes and in the outline's list.
        XSmall,
        /// Between an icon or a mark and its words, such as a sidebar row's or a file's in a tree; at a sidebar row's
        /// ends and around the sidebar's list; above and below the chat's rows; between a comment's Cancel and Save;
        /// and above and below a prompt waiting in the queue or a question's option.
        Small,
        /// Around what's in a card, such as the update card, the workspace's card or a turn's card, and between the
        /// parts of the update card or a question; at both ends of Changes' bar, the pull request's bar and a file's
        /// path, and where a tab starts; at the sides of a call's block; and between an icon and its words in Settings'
        /// pages and Home's rows.
        Medium,
        /// At the sides of the chat's column and under the composer; where the workspace's header and Home's bar start,
        /// and at both ends of an image's comparison's bar, a subagent's bar and Home's rows; around Accounts' panel;
        /// and between the settings on a page and the parts of an account's row.
        Large,
        /// Around what's in Accounts' panel, and around a picture in the file viewer or an image's comparison; under a
        /// setting; between the two sides in the bar under an image's comparison's; and the least room between the two
        /// ends of a line laid out to its ends, such as a setting's name and its switch.
        XLarge,
        /// At the sides of a Settings page, and between its lists, such as Accounts'.
        XXLarge,
    }
}

impl Space {
    /// How much room it is.
    pub const fn size(self) -> Pixels {
        match self {
            Space::XSmall => px(4.),
            Space::Small => px(8.),
            Space::Medium => px(12.),
            Space::Large => px(16.),
            Space::XLarge => px(24.),
            Space::XXLarge => px(32.),
        }
    }

    /// How the spec's table writes it: its name and how much room it is.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.size()))]
    }
}

/// So a gap or a padding takes a step by its name, as `.gap(Space::Small)`.
impl From<Space> for DefiniteLength {
    fn from(space: Space) -> Self {
        space.size().into()
    }
}

/// So a margin takes a step by its name, as `.mt(Space::Small)`.
impl From<Space> for Length {
    fn from(space: Space) -> Self {
        space.size().into()
    }
}
