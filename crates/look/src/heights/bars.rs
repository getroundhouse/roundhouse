use crate::traits::Reach;
use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How tall a bar is, from one edge of a pane or a card to the other, with an edge under it. They're sizes, ordered
    /// from the smallest, and each is chosen by where a bar sits. A bar holds buttons of its own size, with room above
    /// and below them. What else is called a bar, such as the find bar or the composer's, isn't one of these.
    BarSize {
        /// Under a bar, saying more of what that one shows, in a detail's text, such as what each side of an image's
        /// comparison is.
        Small,
        /// Along the top of a pane or a card: the workspace's header, the pull request's bar, Home's bar, both rows of
        /// tabs, the bar over Changes' files that sends the comments waiting, a file's path, a subagent's tab's bar,
        /// and the new workspace's card's. Every one is as tall, so the bars side by side across the window line up,
        /// and so is the strip along the window's top that they're in.
        Regular,
    }
}

impl BarSize {
    /// How tall it is, the edge under it included.
    pub fn height(self) -> Pixels {
        self.height_at(Reach::now())
    }

    /// The same at `reach`.
    pub const fn height_at(self, reach: Reach) -> Pixels {
        match reach {
            Reach::Desk => match self {
                BarSize::Small => px(32.),
                BarSize::Regular => px(40.),
            },
            Reach::Handheld => match self {
                BarSize::Small => px(36.),
                BarSize::Regular => px(44.),
            },
        }
    }

    /// How the spec's table writes it: its name and how tall it is.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 3] {
        let pixels = |reach| format!("{}px", f32::from(self.height_at(reach)));
        [format!("{self:?}"), pixels(Reach::Desk), pixels(Reach::Handheld)]
    }
}
