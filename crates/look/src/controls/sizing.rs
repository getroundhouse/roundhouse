use crate::named::named;
use crate::typography::Type;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::{Pixels, Styled as _, px};

named! {
    /// How big a button is: how tall, the room at each end of its words, and how big its words and icon are.
    /// [`Sizing::sized`] sizes a button with words as one, and [`Sizing::sized_square`] one that's only an icon.
    ControlSize {
        /// Set in a line of text or a row one line tall: a row of a list, a tab, the heading over a list, a call's
        /// line in the chat, a line of a card's text, the composer's bar, the find bar, setup's strip and a fold, and
        /// the window's own buttons in its corner, beside the Mac's. A switch's segments are small, so the switch, with
        /// the room around them, is as tall as a regular button.
        Small,
        /// A bar's own, along a pane's edge, such as a header's, a tab bar's, the pull request's bar's or Home's bar's,
        /// or a card's or a page's own action, set apart from its text, such as the update card's, a comment's on a
        /// diff, a setting's or an account's, and the button over the chat back to its end.
        Regular,
        /// On its own: a dialog's buttons, the one thing an empty pane says to do, and what answers a card in the chat,
        /// such as approving a plan or retrying a request.
        Large,
    }
}

impl ControlSize {
    /// How tall it is, and how wide too, for one that's only an icon.
    pub const fn height(self) -> Pixels {
        match self {
            ControlSize::Small => px(24.),
            ControlSize::Regular => px(28.),
            ControlSize::Large => px(32.),
        }
    }

    /// The room at each end of a button with words, between its edge and its words. Something that lines the words up
    /// with an edge past the button, such as a quiet button under a card's text, takes the button out by this.
    pub const fn padding(self) -> Pixels {
        match self {
            ControlSize::Small => px(8.),
            ControlSize::Regular => px(12.),
            ControlSize::Large => px(16.),
        }
    }

    /// How far a square's own icon is in from each of its edges. Something that lines the icon up with an edge past
    /// the square, such as the arrows over a question, takes the square out by this.
    pub fn inset(self) -> Pixels {
        (self.height() - self.icon()) / 2.
    }

    /// The kind of text a button's words are: a detail for a small one, since it's set among text, and body text for
    /// the others.
    pub const fn words(self) -> Type {
        match self {
            ControlSize::Small => Type::Detail,
            ControlSize::Regular | ControlSize::Large => Type::Body,
        }
    }

    /// How big a square's own icon is.
    pub(super) const fn icon(self) -> Pixels {
        match self {
            ControlSize::Small | ControlSize::Regular => px(14.),
            ControlSize::Large => px(16.),
        }
    }

    /// The kit's size whose words, and icon beside them, a button with words takes, which are [`Self::words`], with a
    /// 12px, 14px or 16px icon. The kit sets a button's words from its size on what it draws inside the button, which
    /// a style from outside doesn't reach, so the size is the kit's, and a test holds its text to [`Self::words`].
    pub(crate) const fn kit_words(self) -> Size {
        match self {
            ControlSize::Small => Size::XSmall,
            ControlSize::Regular => Size::Small,
            ControlSize::Large => Size::Medium,
        }
    }

    /// The kit's size whose icon a button that's only an icon takes, which is [`Self::icon`].
    pub(crate) const fn kit_icon(self) -> Size {
        match self {
            ControlSize::Small | ControlSize::Regular => Size::Small,
            ControlSize::Large => Size::Medium,
        }
    }

    /// How the spec's table writes it: its name, how tall it is, the room beside its words, and their kind of text.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 4] {
        let pixels = |pixels: Pixels| format!("{}px", f32::from(pixels));
        [format!("{self:?}"), pixels(self.height()), pixels(self.padding()), format!("{:?}", self.words())]
    }
}

/// Sizes a button as one of the sizes a button comes in, rather than as one of the kit's. It's the kit's button's
/// alone: a field or a switch takes its size from the kit by rules of its own, which the components will replace.
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
        self.with_size(size.kit_words()).h(size.height()).px(size.padding())
    }

    fn sized_square(self, size: ControlSize) -> Self {
        self.with_size(size.kit_icon()).size(size.height()).p_0()
    }
}
