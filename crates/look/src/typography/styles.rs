use crate::named::named;
use crate::traits::Reach;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AbsoluteLength, App, DefiniteLength, FontStyle, FontWeight, Pixels, SharedString, StrikethroughStyle, Styled,
    UnderlineStyle, px, rems,
};

/// How big a rem is at a desk. The kit's theme is given it, and the window measures the kinds that are in rems by it.
pub(crate) const REM: Pixels = px(16.);
/// How big a rem is on a handheld, where every kind of text has a size of its own in pixels: as big as makes the kit's
/// own text, which is 0.875 rem as body text is at a desk, as big as body text is there.
const HANDHELD_REM: Pixels = px(17. / 0.875);

/// How big a rem is at `reach`.
pub(crate) fn rem(reach: Reach) -> Pixels {
    match reach {
        Reach::Desk => REM,
        Reach::Handheld => HANDHELD_REM,
    }
}

/// How big code is at `reach`, which the kit's theme is given too.
pub(crate) fn mono_size(reach: Reach) -> Pixels {
    Type::Code.set_at(reach).size.to_pixels(rem(reach))
}

/// How the app's own text that stands out from what's around it is set: medium, never bold, which shouts in a dense
/// layout, as a card's title or a page's is.
pub(super) const EMPHASIS: FontWeight = MEDIUM_AS_FOUND;
/// How what someone wrote marks as strong is set, such as the agent's `**bold**` and its Markdown headings: semibold,
/// a step past the app's own emphasis, so a line its writer meant to stand out does among the app's quieter titles.
pub const STRONG: FontWeight = FontWeight::SEMIBOLD;

/// How big inline code is, such as a path in a reply, as a share of the size of the text around it.
pub const INLINE_CODE: f32 = 0.875;
/// How big the headings of what someone wrote in Markdown are, such as the agent's `#` and `##`, at each level from 1
/// to 6, as shares of the size of the text they're in. They step down gently from 1.8 and stand out by their weight,
/// [`STRONG`], so they keep their places against the text at any size, and aren't kinds of the app's own text.
pub const MARKDOWN_HEADINGS: [f32; 6] = [1.8, 4. / 3., 17. / 15., 1., 14. / 15., 13. / 15.];
/// How far apart the items of a list someone wrote are, as a share of the size of its text: less than paragraphs are.
pub const LIST_ITEM_GAP: f32 = 0.25;

/// Medium, as the text system finds it. On a Mac, font-kit, which GPUI finds fonts with, reads the system font's Medium
/// face as a little under 530, where CSS puts medium at 500, and finds a face for 500 among those at or below it, which
/// is Regular. Asking for a little over 500 finds the first face at or above it, which is Medium. Elsewhere medium is
/// 500. #1951 tracks the fix to font-kit's table, after which this is 500 everywhere.
#[cfg(target_os = "macos")]
const MEDIUM_AS_FOUND: FontWeight = FontWeight(510.);
#[cfg(not(target_os = "macos"))]
const MEDIUM_AS_FOUND: FontWeight = FontWeight::MEDIUM;

/// The fonts text is set in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Font {
    /// The system font, which the window sets everything in, as the family the kit's theme names. On a Linux desktop
    /// without the font GPUI gives `.SystemUIFont`, that's the installed family GPUI falls back to, which keeps its
    /// medium face. Text that named `.SystemUIFont` itself would fall back to a family by its name alone, in its
    /// regular face, whatever weight it asked for. So it's never named on what's set, and is left to the window.
    System,
    /// The code font, which goes on what's code, and never on what holds other text, since that would take it too.
    /// It's the family the kit's theme names, which the kit picks for the platform from those installed. A family
    /// named here would be one some platforms don't have, where GPUI falls back to a proportional font, in which a
    /// terminal's grid spaces every letter as wide as an `m`.
    Code,
}

/// How a kind of text is set.
#[derive(Clone, Copy)]
pub(super) struct Set {
    pub(super) font: Font,
    pub(super) size: AbsoluteLength,
    /// A weight of its own, or none to leave it to what's around it.
    pub(super) weight: Option<FontWeight>,
    /// How far apart its lines are, or none to leave them to where it's set, as its weight is: what's read takes its
    /// lines from what it's in, such as a field, a menu's row or a diff, and code from
    /// [`CodeLines`](super::CodeLines).
    pub(super) line: Option<DefiniteLength>,
}

impl Type {
    pub(super) fn set(self) -> Set {
        self.set_at(Reach::now())
    }

    /// How it's set at `reach`. At a desk, every kind is in rems, so the kit's own text agrees, and none drifts from
    /// the rest if the rem changes. On a handheld, each is in pixels, as iOS's own kinds of text are, from their
    /// sizes. *Unconfirmed.*
    ///
    /// A key's lines are as tall as what it says, since it's set on its cap, and what's held with it is on the key's
    /// line.
    pub(super) fn set_at(self, reach: Reach) -> Set {
        let read = |size: AbsoluteLength| Set { font: Font::System, size, weight: None, line: None };
        let on_a_key = |size: AbsoluteLength, key: AbsoluteLength| Set { line: Some(key.into()), ..read(size) };
        match reach {
            Reach::Desk => {
                let key = rems(0.625).into();
                match self {
                    Type::PageTitle => Set { weight: Some(EMPHASIS), ..read(rems(1.5).into()) },
                    Type::Heading => Set { weight: Some(EMPHASIS), ..read(rems(1.).into()) },
                    Type::Body => read(rems(0.875).into()),
                    Type::Detail => read(rems(0.75).into()),
                    Type::Code => Set { font: Font::Code, ..read(rems(0.75).into()) },
                    Type::Key => on_a_key(key, key),
                    Type::KeyHeld => on_a_key(rems(0.46875).into(), key),
                }
            }
            Reach::Handheld => {
                let key = px(12.).into();
                match self {
                    Type::PageTitle => Set { weight: Some(EMPHASIS), ..read(px(28.).into()) },
                    Type::Heading => Set { weight: Some(EMPHASIS), ..read(px(17.).into()) },
                    Type::Body => read(px(17.).into()),
                    Type::Detail => read(px(15.).into()),
                    Type::Code => Set { font: Font::Code, ..read(px(14.).into()) },
                    Type::Key => on_a_key(key, key),
                    Type::KeyHeld => on_a_key(px(9.).into(), key),
                }
            }
        }
    }

    /// Its font, for code that names one itself, such as [`LineText`](crate::LineText), which measures text to put
    /// it on a baseline: the system font or the code font, as the kit's theme names it.
    pub fn family(self, cx: &App) -> SharedString {
        match self.set().font {
            Font::System => cx.theme().font_family.clone(),
            Font::Code => cx.theme().mono_font_family.clone(),
        }
    }

    /// How big it is, in a window whose rem is `rem`.
    pub fn size(self, rem: Pixels) -> Pixels {
        self.set().size.to_pixels(rem)
    }

    /// How far apart its lines are where a place sets them `share` of its size apart, such as a question: at the size
    /// it's set at for the reach this process draws for, so they follow the reach as its size does. They're in whole
    /// pixels, which the text system's rounding to the screen's pixels leaves as they are, so what's beside a line and
    /// as tall, such as a question's number, is as tall as the line on every screen.
    pub fn lines(self, share: f32) -> Pixels {
        (self.set().size.to_pixels(rem(Reach::now())) * share).round()
    }

    /// How big it is at `reach`, in a window whose rem is `rem`, for a test of what has to fit somewhere.
    #[cfg(test)]
    pub(crate) fn size_at(self, reach: Reach, rem: Pixels) -> Pixels {
        self.set_at(reach).size.to_pixels(rem)
    }

    /// How far apart its lines are at `reach`, if it has lines of its own.
    #[cfg(test)]
    pub(crate) fn line_at(self, reach: Reach) -> Option<Pixels> {
        let set = self.set_at(reach);
        set.line.map(|line| line.to_pixels(set.size, rem(reach)))
    }

    /// How the spec's table writes it: its name, its font, its size at a desk, at the app's rem there, its weight, and
    /// its size on a handheld.
    ///
    /// # Panics
    ///
    /// For a weight the table has no word for.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 5] {
        let font = match self.set().font {
            Font::System => "System",
            Font::Code => "Code",
        };
        let weight = match self.set().weight {
            None => "As around it",
            Some(EMPHASIS) => "Medium",
            Some(weight) => panic!("{self:?} is {weight:?}, which the table has no word for"),
        };
        let size = |reach| format!("{}px", f32::from(self.set_at(reach).size.to_pixels(rem(reach))));
        [format!("{self:?}"), font.to_string(), size(Reach::Desk), weight.to_string(), size(Reach::Handheld)]
    }
}

/// Sets `element`'s text as big as `set` has it, with any lines and weight of its own.
fn sized<S: Styled>(element: S, set: Set) -> S {
    let element = element.text_size(set.size);
    let element = match set.line {
        Some(line) => element.line_height(line),
        None => element,
    };
    match set.weight {
        Some(weight) => element.font_weight(weight),
        None => element,
    }
}

/// Sets text as one of the kinds the app has.
pub trait Typeset: Styled + Sized {
    /// Sets this element's text as `kind`, in the system font: its size, and any lines and weight of its own. The
    /// system font is the window's, so it isn't named. Code is set with [`typeset_code`](Self::typeset_code), since its
    /// font is the one the kit's theme names, which takes the app to read, and a `kind` of code here is set in the
    /// system font, which a debug build refuses.
    #[must_use]
    fn typeset(self, kind: Type) -> Self {
        debug_assert!(kind.set().font == Font::System, "{kind:?} is set with `typeset_code`, in the theme's font");
        sized(self, kind.set())
    }

    /// Sets this element's text as code: its size, any lines and weight of its own, and the code font, as the kit's
    /// theme names it.
    #[must_use]
    fn typeset_code(self, cx: &App) -> Self {
        sized(self, Type::Code.set()).font_family(Type::Code.family(cx))
    }

    /// Makes this element's text stand out from what's around it, as a name that needs a look or a card's title does.
    #[must_use]
    fn emphasis(self) -> Self {
        self.font_weight(EMPHASIS)
    }

    /// Sets this element's text apart from the text it's drawn among: regular, upright, and with no line under or
    /// through it, each of which it would otherwise take from that text, as anything does from what it's in. So a
    /// component drawn among a run of formatted words, such as a chip in a bold link, with what it opens, is drawn as
    /// it is anywhere else. GPUI clears a line only by making it no thickness.
    #[must_use]
    fn apart_from_its_text(mut self) -> Self {
        let text = self.text_style();
        text.font_weight = Some(FontWeight::NORMAL);
        text.font_style = Some(FontStyle::Normal);
        text.underline = Some(UnderlineStyle { thickness: px(0.), ..UnderlineStyle::default() });
        text.strikethrough = Some(StrikethroughStyle { thickness: px(0.), ..StrikethroughStyle::default() });
        self
    }
}

impl<E: Styled> Typeset for E {}

named! {
    /// A kind of text, by what it's for. [`Typeset::typeset`] sets an element's text as one.
    Type {
        /// A page's name, such as each of Settings' pages: the largest kind, since it says where you are.
        PageTitle,
        /// What an empty pane says is missing or went wrong, above why.
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
