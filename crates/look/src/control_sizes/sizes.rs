use gpui_kit::{Pixels, px};

use crate::traits::Reach;

use crate::icon_sizes::IconSize;
use crate::named::named;
use crate::space::Space;
use crate::typography::Type;

named! {
    /// How big a control is, such as a button or a field: how tall, the room at each end of what it says, and how
    /// big that is. [`crate::Sizing::sized`] sizes a button with words as one, and [`crate::Sizing::sized_square`] one
    /// that's only an icon.
    ControlSize {
        /// Set in a line of text or a row one line tall: a row of a list, a tab, the heading over a list, a call's
        /// line in the chat, a line of a card's text, the composer's bar, the find bar, a banner and a fold, and
        /// the window's own buttons in its corner, beside the Mac's. A switch's segments are small, so the switch, with
        /// the room around them, is as tall as a regular button.
        Small,
        /// A bar's own, along a pane's edge, such as a header's, a tab bar's, the pull request's bar's or Home's bar's,
        /// or a card's or a page's own action, set apart from its text, such as the update card's, a comment's on a
        /// diff, a setting's or an account's, and the button over the chat back to its end.
        Regular,
        /// On its own: a dialog's buttons, a menu's rows, the one thing an empty pane says to do, and what answers a
        /// card in the chat, such as approving a plan or retrying a request.
        Large,
    }
}

impl ControlSize {
    /// How tall it is, and how wide too, for one that's only an icon.
    pub fn height(self) -> Pixels {
        self.height_at(Reach::now())
    }

    /// How tall it is at `reach`.
    pub const fn height_at(self, reach: Reach) -> Pixels {
        match reach {
            Reach::Desk => match self {
                ControlSize::Small => px(24.),
                ControlSize::Regular => px(28.),
                ControlSize::Large => px(32.),
            },
            Reach::Handheld => match self {
                ControlSize::Small => px(32.),
                ControlSize::Regular => px(36.),
                ControlSize::Large => px(44.),
            },
        }
    }

    /// The room at each end of a control, between its edge and what it says, such as a button's words. It's a step of
    /// space, so it's the same at both reaches, as the owner decided. Something that lines the words up with an edge
    /// past the button, such as a quiet button under a card's text, takes the button out by this.
    pub const fn padding(self) -> Pixels {
        match self {
            ControlSize::Small => Space::Small.size(),
            ControlSize::Regular => Space::Medium.size(),
            ControlSize::Large => Space::Large.size(),
        }
    }

    /// How far a square's own icon is in from each of its edges. Something that lines the icon up with an edge past
    /// the square, such as the arrows over a question, takes the square out by this.
    pub fn inset(self) -> Pixels {
        (self.height() - self.icon().size()) / 2.
    }

    /// The kind of text what a control says is, such as a button's words or what's typed in a field: a detail for a
    /// small one, since it's set among text, and body text for the others.
    pub const fn text(self) -> Type {
        match self {
            ControlSize::Small => Type::Detail,
            ControlSize::Regular | ControlSize::Large => Type::Body,
        }
    }

    /// How big a square's own icon is, or an icon set in a square of this size, such as the one that opens closed
    /// chats.
    pub const fn icon(self) -> IconSize {
        match self {
            ControlSize::Small | ControlSize::Regular => IconSize::Regular,
            ControlSize::Large => IconSize::Large,
        }
    }

    /// How big an icon beside a button's words is, or one set among them, such as the arrow after a pull request's
    /// number.
    pub const fn icon_with_words(self) -> IconSize {
        match self {
            ControlSize::Small => IconSize::Small,
            ControlSize::Regular => IconSize::Regular,
            ControlSize::Large => IconSize::Large,
        }
    }

    /// How the spec's table writes it: its name, how tall it is at a desk, the room at its ends, its kind of text, and
    /// how tall it is on a handheld.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 5] {
        let pixels = |pixels: Pixels| format!("{}px", f32::from(pixels));
        [
            format!("{self:?}"),
            pixels(self.height_at(Reach::Desk)),
            pixels(self.padding()),
            format!("{:?}", self.text()),
            pixels(self.height_at(Reach::Handheld)),
        ]
    }
}
