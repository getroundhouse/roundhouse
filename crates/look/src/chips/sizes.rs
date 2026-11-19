use gpui_kit::{App, Pixels, Window, px};

use crate::icon_sizes::IconSize;
use crate::named::named;
use crate::typography::Type;

named! {
    /// How big a chip is: how tall, and how big its mark and its words are, each smaller in a smaller chip, so a small
    /// one is a regular one made smaller rather than squeezed. The room at its start and between its parts is a
    /// chip's own, the same at each size. [`crate::chip`] makes one of a size.
    ChipSize {
        /// Set in a line as tall as a small control, such as a call's line, as tall as a pill, so the two sit in one
        /// line.
        Regular,
        /// Set among a paragraph's words, such as an issue a reply names, shorter than a line of them, so a chip on the
        /// line above or below doesn't touch it. It has no lines or what changed, which a regular one says.
        Small,
    }
}

impl ChipSize {
    /// How tall it is.
    pub const fn height(self) -> Pixels {
        match self {
            ChipSize::Regular => crate::pills::HEIGHT,
            ChipSize::Small => px(20.),
        }
    }

    /// The kind of text its words are: its name, and what's said of it.
    pub const fn text(self) -> Type {
        match self {
            ChipSize::Regular => Type::Body,
            ChipSize::Small => Type::Detail,
        }
    }

    /// How big its mark is.
    pub const fn mark(self) -> IconSize {
        match self {
            ChipSize::Regular => IconSize::Regular,
            ChipSize::Small => IconSize::Small,
        }
    }

    /// How far below its top its words' baseline is, as they're centred in it, on a whole device pixel, as GPUI puts a
    /// glyph's, so one set among words, such as in a reply, sits on theirs.
    pub fn baseline(self, window: &Window, cx: &App) -> Pixels {
        let text = self.text();
        let below = crate::baseline::below_middle(&text.family(cx), text.size(window.rem_size()), window);
        let scale = window.scale_factor();
        px(crate::baseline::nearest_pixel((self.height() / 2. + below).scale(scale)).0 / scale)
    }

    /// How the spec's table writes it: its name, how tall it is, its text and its mark.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 4] {
        [
            format!("{self:?}"),
            format!("{}px", f32::from(self.height())),
            format!("{:?}", self.text()),
            format!("{:?}", self.mark()),
        ]
    }
}
