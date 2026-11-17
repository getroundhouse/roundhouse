use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How wide a dialog is, by how much it holds. [`crate::Dialog::width`] sets one.
    DialogWidth {
        /// A question and its answers, with nothing in it, such as closing a chat whose agent is working.
        Small,
        /// What most dialogs hold: a field, a line or two and its answers, such as renaming a chat.
        Regular,
        /// What a line of its own runs across, or three answers, such as sharing the hub's link.
        Wide,
    }
}

impl DialogWidth {
    /// How wide it is, unless the window is narrower.
    pub fn width(self) -> Pixels {
        match self {
            DialogWidth::Small => px(360.),
            DialogWidth::Regular => px(448.),
            DialogWidth::Wide => px(512.),
        }
    }

    /// How the spec's table writes it: its name and its width.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.width()))]
    }
}
