use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How wide a card is. One over the rest has a width of its own, by how much it holds, since nothing around it
    /// sets one, and one in place or in the chat fills what it's set in. [`crate::card`] makes one of a width.
    CardWidth {
        /// A few lines, such as a hover card, the context's card or a turn's.
        Small,
        /// More, or what runs across a line of its own, such as the workspace's card or dictation's offer.
        Regular,
        /// As wide as what it's set in, such as the chat's column, a page in Settings or the sidebar's foot.
        Fills,
    }
}

impl CardWidth {
    /// How wide it is, for one with a width of its own.
    pub fn width(self) -> Option<Pixels> {
        match self {
            CardWidth::Small => Some(px(300.)),
            CardWidth::Regular => Some(px(380.)),
            CardWidth::Fills => None,
        }
    }

    /// How the spec's table writes it: its name and its width.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        let width =
            self.width().map_or_else(|| "What it's set in".to_string(), |width| format!("{}px", f32::from(width)));
        [format!("{self:?}"), width]
    }
}
