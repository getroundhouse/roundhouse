use crate::named::named;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AbsoluteLength, App, FontWeight, Pixels, SharedString, Styled, px, rems};

/// How big a rem is. The kit's theme is given it, and the window measures the kinds that are in rems by it.
pub(crate) const REM: Pixels = px(16.);
/// The code font. Every Mac has it, and Linux and Windows don't, so code there falls back to the system font.
pub(crate) const MONO: &str = "Menlo";
/// How big code is, which the kit's theme is given too.
pub(crate) const MONO_SIZE: Pixels = px(12.);
/// How text that stands out from what's around it is set: medium, never bold, which shouts in a dense layout.
const EMPHASIS: FontWeight = FontWeight::MEDIUM;

/// The fonts text is set in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Font {
    /// The system font, which the window sets everything in, as the family the kit's theme names. On a Linux desktop
    /// without the font GPUI gives `.SystemUIFont`, that's the installed family GPUI falls back to, which keeps its
    /// medium face. Text that named `.SystemUIFont` itself would fall back to a family by its name alone, in its
    /// regular face, whatever weight it asked for. So it's never named on what's set, and is left to the window.
    System,
    /// The code font, which goes on what's code, and never on what holds other text, since that would take it too.
    Code,
}

/// How a kind of text is set.
#[derive(Clone, Copy)]
pub(super) struct Set {
    pub(super) font: Font,
    pub(super) size: AbsoluteLength,
    /// A weight of its own, or none to leave it to what's around it.
    pub(super) weight: Option<FontWeight>,
}

impl Type {
    pub(super) fn set(self) -> Set {
        let system = |size: AbsoluteLength| Set { font: Font::System, size, weight: None };
        match self {
            Type::PageTitle => Set { weight: Some(EMPHASIS), ..system(px(24.).into()) },
            Type::Heading => Set { weight: Some(EMPHASIS), ..system(rems(1.).into()) },
            Type::Body => system(rems(0.875).into()),
            Type::Detail => system(rems(0.75).into()),
            Type::Code => Set { font: Font::Code, size: MONO_SIZE.into(), weight: None },
            Type::Key => system(px(10.).into()),
            Type::KeyHeld => system(px(7.5).into()),
        }
    }

    /// Its font, for code that names one itself, such as [`LineText`](crate::LineText), which measures text to put
    /// it on a baseline: the system font as the kit's theme names it, or the code font.
    pub fn family(self, cx: &App) -> SharedString {
        match self.set().font {
            Font::System => cx.theme().font_family.clone(),
            Font::Code => SharedString::new_static(MONO),
        }
    }

    /// How big it is, in a window whose rem is `rem`.
    pub fn size(self, rem: Pixels) -> Pixels {
        self.set().size.to_pixels(rem)
    }

    /// How the spec's table writes it: its name, its font, its size at the app's rem, and its weight.
    ///
    /// # Panics
    ///
    /// For a weight the table has no word for.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 4] {
        let font = match self.set().font {
            Font::System => "System",
            Font::Code => MONO,
        };
        let weight = match self.set().weight {
            None => "As around it",
            Some(EMPHASIS) => "Medium",
            Some(weight) => panic!("{self:?} is {weight:?}, which the table has no word for"),
        };
        let size = format!("{}px", f32::from(self.size(REM)));
        [format!("{self:?}"), font.to_string(), size, weight.to_string()]
    }
}

/// Sets text as one of the kinds the app has.
pub trait Typeset: Styled + Sized {
    /// Sets this element's text as `kind`: its size, any weight of its own, and the code font for code. Text in the
    /// system font takes the window's.
    #[must_use]
    fn typeset(self, kind: Type) -> Self {
        let set = kind.set();
        let element = self.text_size(set.size);
        let element = match set.font {
            Font::System => element,
            Font::Code => element.font_family(SharedString::new_static(MONO)),
        };
        match set.weight {
            Some(weight) => element.font_weight(weight),
            None => element,
        }
    }

    /// Makes this element's text stand out from what's around it, as a name that needs a look or a card's title does.
    #[must_use]
    fn emphasis(self) -> Self {
        self.font_weight(EMPHASIS)
    }
}

impl<E: Styled> Typeset for E {}

named! {
    /// A kind of text, by what it's for. [`Typeset::typeset`] sets an element's text as one.
    Type {
        /// A page's name, such as each of Settings' pages: the largest kind, since it says where you are.
        PageTitle,
        /// What an empty pane says is missing, above why.
        Heading,
        /// What to read: what the agent wrote, a prompt, a call's line, a row's name, and most of the rest.
        Body,
        /// Details beside or under what to read, such as a time, a count, a hint or a heading over a list.
        Detail,
        /// Code, in the code font: a command, a path, a file, or what a call put out.
        Code,
        /// What a key says, such as the `F` of Settings' `⌘ F`.
        Key,
        /// What's held with a key, such as Settings' `⌘`, smaller than the key that's pressed, as on the keys
        /// themselves.
        KeyHeld,
    }
}
