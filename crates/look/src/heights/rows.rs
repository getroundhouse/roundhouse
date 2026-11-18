use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How tall a row of a list is that's one line tall, with room above and below it. They're sizes, ordered from the
    /// smallest, and each is chosen by what the list is, so the rows of the same kind of list are as tall, wherever
    /// that is. Most lists are regular, and small is used sparingly, for a long list to look down, and whatever the
    /// list, for the heading over it and a line on its own, set apart from a list. A row holds a small button, with
    /// room above and below. A list that places its rows itself chooses their size once, and the rows it draws take it
    /// from there.
    RowSize {
        /// Used sparingly: a long list to look down, where more room would show less of it at once, such as a file in a
        /// tree, a call's line in the chat, a page a search found or a closed chat in its menu, and the heading over
        /// any list, and a line on its own, set apart from a list, such as a fold among a diff's lines or the build
        /// line under the sidebar.
        Small,
        /// Most lists, such as the places to go in a sidebar: the sidebar's own places, its projects and their
        /// workspaces, and Settings' pages.
        Regular,
        /// A row of a table: Home's workspaces, and the accounts in Settings' Agents and Git, which have a line between
        /// each and the next.
        Large,
    }
}

impl RowSize {
    /// How tall it is, with any line drawn under it, such as the one between a table's rows.
    pub const fn height(self) -> Pixels {
        match self {
            RowSize::Small => px(28.),
            RowSize::Regular => px(36.),
            RowSize::Large => px(44.),
        }
    }

    /// How the spec's table writes it: its name and how tall it is.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.height()))]
    }
}
