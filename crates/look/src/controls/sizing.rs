use gpui_kit::component::button::Button;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::{StyleRefinement, Styled as _};

use crate::ControlSize;
use crate::typography::Typeset as _;

/// The kit's size a button is given for what the kit still sizes from it: the room between its icon and its words,
/// and the caret after the words of one that opens a menu. Its words and icon are sized as [`ControlSize::text`] and
/// [`ControlSize::icon_with_words`] or [`ControlSize::icon`] say, and its height and the room at its ends as this size's own.
const fn kit(size: ControlSize) -> Size {
    match size {
        ControlSize::Small => Size::XSmall,
        ControlSize::Regular => Size::Small,
        ControlSize::Large => Size::Medium,
    }
}

/// Sizes a button as one of a control's sizes, rather than as one of the kit's.
pub trait Sizing {
    /// Sizes a button with words, and maybe an icon beside them, as `size`: how tall it is, the room at each end, and
    /// how big its words and icon are.
    #[must_use]
    fn sized(self, size: ControlSize) -> Self;

    /// Sizes a button that's only an icon as a square of `size`, with its own icon in the middle, as big as `size`
    /// has it. An icon that's the button's child keeps its own size, and is in the middle too.
    #[must_use]
    fn sized_square(self, size: ControlSize) -> Self;
}

impl Sizing for Button {
    fn sized(self, size: ControlSize) -> Self {
        let words = StyleRefinement::default().typeset(size.text());
        let icon = Size::Size(size.icon_with_words().size());
        self.with_size(kit(size)).content_style(words, icon).h(size.height()).px(size.padding())
    }

    fn sized_square(self, size: ControlSize) -> Self {
        let icon = Size::Size(size.icon().size());
        self.with_size(kit(size)).content_style(StyleRefinement::default(), icon).size(size.height()).p_0()
    }
}
