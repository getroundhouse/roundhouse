//! What a terminal shows at one moment, sent as the rows its receiver lacks, so a receiver that has most of the screen
//! is sent only what changed, and one on a slow link is sent the newest of what the terminal shows rather than all that
//! came between.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use super::rows::{CellColor, Numbering, RowNumber, TerminalRow, is_false};
use super::size::TerminalSize;

/// One frame of a picture. A picture too big for a frame is sent in several, each with the same numbering, size, top,
/// first and `kept_from`, and the cursor and state on the last, which is `whole`. A receiver draws only a whole one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TerminalPicture {
    pub numbering: Numbering,
    pub size: TerminalSize,
    /// The number of the screen's first row.
    pub top: RowNumber,
    /// The number of the oldest row of history the terminal keeps.
    pub first: RowNumber,
    /// The rows the receiver lacks, by number, in runs of neighbours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<RowRun>,
    /// The rows the receiver holds above `top` are the terminal's history as it is from this number on. One before it
    /// left the screen after the receiver's copy of it, so the receiver lets go of it and those above. It speaks only
    /// of rows its sender has sent it since it last showed another numbering, or connected. So a receiver lets go of
    /// the rows that were on its screen when it was sent a picture of another numbering, or its connection went, since
    /// they could have changed before they left the screen.
    pub kept_from: RowNumber,
    /// With this frame, the receiver has the screen as it was at one moment.
    #[serde(default, skip_serializing_if = "is_false")]
    pub whole: bool,
    /// Where the cursor is, on a whole picture while it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<TerminalCursor>,
    /// On a whole picture, when it changed since the receiver's last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ScreenState>,
}

/// Rows with numbers one after another.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RowRun {
    pub from: RowNumber,
    pub rows: Vec<RowJson>,
}

/// A row as JSON, so a hub keeps it and passes it on without knowing what's in it, and two copies of a row are compared
/// as text.
#[derive(Clone, Debug)]
pub struct RowJson(Arc<RawValue>);

impl RowJson {
    pub fn new(row: &TerminalRow) -> Result<Self, serde_json::Error> {
        serde_json::value::to_raw_value(row).map(|json| Self(Arc::from(json)))
    }

    /// What it says, or a blank row for one this build can't read.
    #[must_use]
    pub fn row(&self) -> TerminalRow {
        serde_json::from_str(self.0.get()).unwrap_or_default()
    }

    /// How long it is written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.get().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.get().is_empty()
    }
}

impl PartialEq for RowJson {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.get() == other.0.get()
    }
}

impl Eq for RowJson {}

impl Serialize for RowJson {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Read as a JSON value and written again, since a frame's tag comes inside it, so serde holds what it reads of a frame
/// before it knows the frame's kind, and a raw value can't be read from what serde holds.
impl<'de> Deserialize<'de> for RowJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let json = serde_json::value::to_raw_value(&value).map_err(serde::de::Error::custom)?;
        Ok(Self(Arc::from(json)))
    }
}

/// Where the cursor is on the screen, and how it's drawn.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCursor {
    /// Its column, which is a wide character's first when it's on either of its cells.
    pub col: u16,
    /// Its row on the screen, from 0 for the first.
    pub row: u16,
    pub shape: CursorShape,
    #[serde(default, skip_serializing_if = "is_false")]
    pub blinking: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    Block,
    /// A block's outline, as for a terminal that isn't focused.
    BlockHollow,
    Bar,
    Underline,
    /// One a newer peer knows, drawn as a block.
    #[serde(other)]
    Unknown,
}

/// What a terminal is doing that isn't in its rows. Sent whole, whenever any of it changes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenState {
    /// Whether a full-screen program's screen shows, which has no history, rather than the shell's.
    #[serde(default, skip_serializing_if = "is_false")]
    pub program: bool,
    pub wheel: Wheel,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The colours of the palette the program changed, which a client draws in place of its theme's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<PaletteColor>,
    /// The colours of a cell that sets none, and of the cursor, where the program set them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<CellColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<CellColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_color: Option<CellColor>,
    /// Whether the program asked for the whole screen's colours swapped.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
}

/// What a turn of the wheel does, which a client reads to decide at once whether it scrolls its own view.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Wheel {
    /// Scrolls the history.
    #[default]
    Scrolls,
    /// Sent to the program as arrow keys, as a full-screen program's screen, with no history, asks.
    Arrows,
    /// Nothing, since the program asked for the mouse, until the mouse is sent to programs.
    Program,
    /// One a newer peer knows, which scrolls the history.
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteColor {
    #[serde(rename = "i")]
    pub index: u8,
    #[serde(rename = "c")]
    pub color: CellColor,
}
