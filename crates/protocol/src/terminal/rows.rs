//! A terminal's rows, as its worker numbers them and says what they hold, so what's read of a terminal is a row by its
//! number, rather than the bytes that rebuild the whole of it.
//!
//! The field names are a letter each, since a screen of rows is most of what a terminal sends. Each type's are in the
//! order of their letters, the order a JSON value read back writes them in, so a row's text is the same whether its
//! worker wrote it or a hub read it and wrote it again, and two copies of a row can be compared as text.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Where a row is among a terminal's rows in one [`Numbering`], its history and its screen together, from 0 for the
/// first row the terminal had when the numbering began. A number never moves to another row, so a row of history with a
/// given number holds the same for as long as the terminal keeps it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RowNumber(u64);

impl RowNumber {
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The number `count` rows after this.
    #[must_use]
    pub const fn plus(self, count: u64) -> Self {
        Self(self.0 + count)
    }
}

/// Which numbering a terminal's [`RowNumber`]s are in. A new one starts when the terminal is resized, since every line
/// is wrapped again, when its history is cleared or it's reset, and whenever the worker loses count of its rows, so a
/// row number from another numbering names no row. Each is newer than the one before.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Numbering(u64);

impl Numbering {
    pub const fn new(numbering: u64) -> Self {
        Self(numbering)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The one after this.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// What a row of a terminal holds, as runs of cells, each in one style. Cells after the last run are blank, in the
/// terminal's own colours. The same cells always make the same row, so two rows can be compared as they're sent.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TerminalRow {
    #[serde(rename = "s", default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    /// Whether its line goes on in the next row, as a line too long for one does, so the two are one line to select or
    /// copy.
    #[serde(rename = "w", default, skip_serializing_if = "is_false")]
    pub wraps: bool,
    /// The styles its spans name, by their place here.
    #[serde(rename = "y", default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<CellStyle>,
}

/// A run of cells in one style.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// The column it starts at, when that isn't where the run before it ended, or 0 for the first.
    #[serde(rename = "c", default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u16>,
    /// Whether each of its characters takes two cells, as most East Asian characters and emoji do.
    #[serde(rename = "d", default, skip_serializing_if = "is_false")]
    pub wide: bool,
    /// Whether its text is all one cell's, such as a letter with its accents, or emoji joined into one.
    #[serde(rename = "g", default, skip_serializing_if = "is_false")]
    pub cluster: bool,
    /// How many cells with no text it covers, which only a background or a line under them shows.
    #[serde(rename = "n", default, skip_serializing_if = "is_zero")]
    pub blank: u16,
    /// Its text, a character to each cell, or two cells to each if it's `wide`, or one cell's if it's a `cluster`. Empty
    /// for `blank` cells.
    #[serde(rename = "t", default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Its style, as a place in the row's `styles`, or `None` for the terminal's own.
    #[serde(rename = "y", default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u16>,
}

/// How a cell is drawn, with its colours as the program gave them, so each client draws a colour from its own theme.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellStyle {
    #[serde(rename = "a", default, skip_serializing_if = "CellAttributes::is_empty")]
    pub attributes: CellAttributes,
    #[serde(rename = "b", default, skip_serializing_if = "Option::is_none")]
    pub background: Option<CellColor>,
    /// The text's colour, or `None` for the terminal's own.
    #[serde(rename = "f", default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<CellColor>,
    /// The underline's colour, or `None` for the text's.
    #[serde(rename = "k", default, skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<CellColor>,
    /// The line under the text, if there is one.
    #[serde(rename = "u", default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<Underline>,
}

/// A colour as the program gave it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellColor {
    /// One of the terminal's 256, which a client draws from its theme for the first 16. Written as its number.
    Palette(u8),
    /// Written as `rrggbb`.
    Rgb { r: u8, g: u8, b: u8 },
    /// One a newer peer writes in a form this build doesn't know, drawn as the terminal's own.
    Unknown,
}

impl Serialize for CellColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::Palette(index) => serializer.serialize_u8(index),
            Self::Rgb { r, g, b } => serializer.serialize_str(&format!("{r:02x}{g:02x}{b:02x}")),
            Self::Unknown => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for CellColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(color_from(&value).unwrap_or(Self::Unknown))
    }
}

fn color_from(value: &serde_json::Value) -> Option<CellColor> {
    if let Some(index) = value.as_u64() {
        return u8::try_from(index).ok().map(CellColor::Palette);
    }
    let hex = value.as_str().filter(|hex| hex.len() == 6)?;
    let part = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some(CellColor::Rgb { r: part(0)?, g: part(2)?, b: part(4)? })
}

/// What a cell's text is drawn with, as bits, which only grow, and a reader ignores a bit it doesn't know.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CellAttributes(u16);

impl CellAttributes {
    pub const BOLD: Self = Self(1);
    pub const ITALIC: Self = Self(1 << 1);
    pub const FAINT: Self = Self(1 << 2);
    pub const STRIKETHROUGH: Self = Self(1 << 3);
    /// Its colours swapped.
    pub const INVERSE: Self = Self(1 << 4);
    pub const INVISIBLE: Self = Self(1 << 5);
    pub const BLINK: Self = Self(1 << 6);
    pub const OVERLINE: Self = Self(1 << 7);

    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, attributes: Self) -> bool {
        self.0 & attributes.0 == attributes.0
    }

    /// These and `attributes`, if `on`.
    #[must_use]
    pub const fn with(self, attributes: Self, on: bool) -> Self {
        if on { Self(self.0 | attributes.0) } else { self }
    }
}

/// The kind of line under a cell's text.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Underline {
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
    /// One a newer peer knows, drawn as a single line.
    #[serde(other)]
    Unknown,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
pub(super) const fn is_false(value: &bool) -> bool {
    !*value
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
const fn is_zero(value: &u16) -> bool {
    *value == 0
}
