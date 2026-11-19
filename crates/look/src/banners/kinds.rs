use gpui_kit::Hsla;
use gpui_kit::assets::IconName;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// What a banner says, which its colour and its icon follow. [`crate::banner`] makes one of a kind.
    BannerKind {
        /// Something went wrong, such as a change the hub refused: washed in red, with a crossed circle.
        Error,
        /// Something's wrong that may cost something later, such as setup that failed, agents that can't push or an
        /// account using its reserve: washed in amber, with a warning sign.
        Warning,
        /// Something to know, which needs nothing done, such as setup that's running or files left out of a list:
        /// washed grey, with an info sign.
        Note,
    }
}

impl BannerKind {
    /// The colour it's washed in, and its icon's.
    pub(super) fn tone(self, colors: &Colors) -> Hsla {
        match self {
            BannerKind::Error => colors.error,
            BannerKind::Warning => colors.warning,
            BannerKind::Note => colors.text_muted,
        }
    }

    /// The icon at its start.
    pub(super) fn icon(self) -> IconName {
        match self {
            BannerKind::Error => IconName::CircleX,
            BannerKind::Warning => IconName::TriangleAlert,
            BannerKind::Note => IconName::Info,
        }
    }

    /// How the spec's table writes it: its name, and its colour by its name in `look.md`.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        let tone = match self {
            BannerKind::Error => "error",
            BannerKind::Warning => "warning",
            BannerKind::Note => "text_muted",
        };
        [format!("{self:?}"), tone.to_string()]
    }
}
