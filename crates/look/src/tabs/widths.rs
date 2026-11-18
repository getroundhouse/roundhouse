use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How wide a tab is. [`crate::tab`] makes one of a width.
    TabWidth {
        /// Every tab in the bar the same width, with as much of its name as fits, so the row doesn't shift when one
        /// gets its name, such as the workspace's tabs of chats, terminals, a file and a subagent.
        Even,
        /// As wide as its name, for a few tabs whose names don't change, such as the sidebar's All files and Changes.
        Fits,
    }
}

impl TabWidth {
    /// How wide it is, for one with a width of its own.
    pub fn width(self) -> Option<Pixels> {
        match self {
            TabWidth::Even => Some(px(200.)),
            TabWidth::Fits => None,
        }
    }

    /// How the spec's table writes it: its name and its width.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        let width = self.width().map_or_else(|| "Its name's".to_string(), |width| format!("{}px", f32::from(width)));
        [format!("{self:?}"), width]
    }
}
