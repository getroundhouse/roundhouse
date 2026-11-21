//! What a row holds, read cell by cell from Ghostty, by the row's place among the screen's and its history's.

use libghostty_vt::Terminal;
use libghostty_vt::grid_ref::GridRef;
use libghostty_vt::point::{Coordinate, Point};
use libghostty_vt::screen::{CellContentTag, CellWide};
use libghostty_vt::style::{self, StyleColor};
use protocol::terminal::{CellAttributes, CellColor, CellStyle, Numbering, RowNumber, Span, TerminalRow, Underline};
use protocol::worker::{MOST_ROWS, TerminalRowsReply};

use super::Counting;
use super::counting::Numbered;
use crate::{Mirror, MirrorError};

/// About a frame's worth of rows, as JSON, past which a reply has the rows before.
const MOST_BYTES: usize = 64 * 1024;

impl Mirror {
    /// The numbering the screen showing is in, and where its rows are in it, which counting starts with, the first time
    /// it's asked.
    pub(in crate::mirror) fn numbered(&mut self) -> Result<Numbered, MirrorError> {
        let counting = match &mut self.rows {
            Some(counting) => counting,
            None => self.rows.insert(Counting::new(&mut self.terminal)?),
        };
        Ok(counting.now())
    }

    /// Rows of the shell's history by their numbers in `numbering`, as `protocol::worker::TerminalRows` asks.
    pub fn history_rows(
        &mut self,
        numbering: Numbering,
        from: RowNumber,
        count: u32,
    ) -> Result<TerminalRowsReply, MirrorError> {
        let now = self.numbered()?;
        let from = from.max(now.first);
        let mut reply =
            TerminalRowsReply { numbering: now.numbering, first: now.first, top: now.top, from, rows: Vec::new() };
        if !now.shell || numbering != now.numbering {
            return Ok(reply);
        }
        let end = from.plus(u64::from(count.min(MOST_ROWS))).min(now.top);
        let mut bytes = 0;
        let mut number = from;
        while number < end && bytes < MOST_BYTES {
            let row = row(&self.terminal, number.get() - now.first.get())?;
            bytes += serde_json::to_vec(&row).map_or(0, |json| json.len());
            reply.rows.push(row);
            number = number.plus(1);
        }
        self.history_read();
        Ok(reply)
    }
}

/// The row `y` rows from the oldest the terminal keeps, of history and then of the screen showing.
pub(in crate::mirror) fn row(terminal: &Terminal<'_, '_>, y: u64) -> Result<TerminalRow, MirrorError> {
    let y = u32::try_from(y).unwrap_or(u32::MAX);
    let mut building = Building::default();
    let mut wraps = false;
    for x in 0..terminal.cols()? {
        let at = terminal.grid_ref(Point::Screen(Coordinate { x, y }))?;
        if x == 0 {
            wraps = at.row()?.is_wrapped()?;
        }
        let cell = at.cell()?;
        let wide = match cell.wide()? {
            CellWide::Narrow => false,
            CellWide::Wide => true,
            // The second cell of the character before, or the cell a wide character that didn't fit left at the end of
            // the row it wrapped from, neither of which holds anything of its own.
            CellWide::SpacerTail | CellWide::SpacerHead => continue,
        };
        let (text, cluster) = match cell.content_tag()? {
            CellContentTag::Codepoint => {
                (char::from_u32(cell.codepoint()?).filter(|c| *c != '\0').map(String::from), false)
            }
            CellContentTag::CodepointGrapheme => (Some(graphemes(&at)?), true),
            CellContentTag::BgColorPalette | CellContentTag::BgColorRgb => (None, false),
        };
        let mut style = cell_style(at.style()?);
        match cell.content_tag()? {
            CellContentTag::BgColorPalette => style.background = Some(CellColor::Palette(cell.bg_color_palette()?.0)),
            CellContentTag::BgColorRgb => {
                let rgb = cell.bg_color_rgb()?;
                style.background = Some(CellColor::Rgb { r: rgb.r, g: rgb.g, b: rgb.b });
            }
            CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => {}
        }
        building.cell(x, text, wide, cluster, style);
    }
    let mut row = building.row;
    row.wraps = wraps;
    Ok(row)
}

/// The whole of a cell's cluster, which can be longer than any buffer guessed at.
fn graphemes(at: &GridRef<'_>) -> Result<String, MirrorError> {
    let mut buffer = vec!['\0'; 8];
    loop {
        match at.graphemes(&mut buffer) {
            Ok(written) => return Ok(buffer[..written].iter().collect()),
            Err(libghostty_vt::Error::OutOfSpace { required }) if required > buffer.len() => {
                buffer.resize(required, '\0');
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn cell_style(style: style::Style) -> CellStyle {
    let attributes = CellAttributes::default()
        .with(CellAttributes::BOLD, style.bold)
        .with(CellAttributes::ITALIC, style.italic)
        .with(CellAttributes::FAINT, style.faint)
        .with(CellAttributes::STRIKETHROUGH, style.strikethrough)
        .with(CellAttributes::INVERSE, style.inverse)
        .with(CellAttributes::INVISIBLE, style.invisible)
        .with(CellAttributes::BLINK, style.blink)
        .with(CellAttributes::OVERLINE, style.overline);
    let underline = match style.underline {
        style::Underline::None => None,
        style::Underline::Single => Some(Underline::Single),
        style::Underline::Double => Some(Underline::Double),
        style::Underline::Curly => Some(Underline::Curly),
        style::Underline::Dotted => Some(Underline::Dotted),
        style::Underline::Dashed => Some(Underline::Dashed),
        _ => Some(Underline::Unknown),
    };
    CellStyle {
        foreground: color(style.fg_color),
        background: color(style.bg_color),
        attributes,
        underline,
        underline_color: color(style.underline_color),
    }
}

fn color(color: StyleColor) -> Option<CellColor> {
    match color {
        StyleColor::None => None,
        StyleColor::Palette(index) => Some(CellColor::Palette(index.0)),
        StyleColor::Rgb(rgb) => Some(CellColor::Rgb { r: rgb.r, g: rgb.g, b: rgb.b }),
    }
}

/// A row as its cells are read, left to right, each joined to the span before it where they're alike.
#[derive(Default)]
struct Building {
    row: TerminalRow,
    /// The column after the last cell taken in.
    next_col: u16,
}

impl Building {
    fn cell(&mut self, col: u16, text: Option<String>, wide: bool, cluster: bool, style: CellStyle) {
        let style = (style != CellStyle::default()).then(|| self.style(style));
        let cells = if wide { 2 } else { 1 };
        // A cell with nothing in it and nothing to draw ends the span before, and leaves a gap.
        if text.is_none() && style.is_none() {
            return;
        }
        let next = col == self.next_col;
        self.next_col = col + cells;
        let joins = next && !cluster;
        if let Some(last) = self.row.spans.last_mut().filter(|last| joins && last.style == style && !last.cluster) {
            match &text {
                Some(text) if !last.text.is_empty() && last.wide == wide => {
                    last.text.push_str(text);
                    return;
                }
                None if last.text.is_empty() => {
                    last.blank += cells;
                    return;
                }
                Some(_) | None => {}
            }
        }
        let col = (!next).then_some(col);
        let blank = if text.is_none() { cells } else { 0 };
        let text = text.unwrap_or_default();
        self.row.spans.push(Span { col, text, blank, style, wide, cluster });
    }

    /// The place of `style` among the row's, added if it's new.
    fn style(&mut self, style: CellStyle) -> u16 {
        let styles = &mut self.row.styles;
        let place = styles.iter().position(|known| *known == style).unwrap_or_else(|| {
            styles.push(style);
            styles.len() - 1
        });
        u16::try_from(place).unwrap_or(u16::MAX)
    }
}
