//! What the worker's copy shows now, for a picture: the screen's rows by number, the cursor, and what the terminal is
//! doing that isn't in its rows.

use libghostty_vt::color::Rgb;
use libghostty_vt::mode::Mode;
use libghostty_vt::render::{CursorVisualStyle, Dirty, RenderState};
use libghostty_vt::screen::Screen;
use protocol::terminal::{
    CellColor, CursorShape, PaletteColor, RowJson, RowNumber, ScreenState, Shown, TerminalCursor, TerminalSize, Wheel,
};

use super::reading::{Place, row};
use crate::{Mirror, MirrorError};

/// Ghostty's own reading of what it shows, which alone says how the cursor is drawn. Only the worker's copy has one,
/// and only once it's asked for a picture.
#[derive(Debug)]
pub(in crate::mirror) struct Picturing {
    render: RenderState<'static>,
}

impl Mirror {
    /// What it shows now, in the numbering the screen showing is in.
    pub fn shown(&mut self) -> Result<Shown, MirrorError> {
        let now = self.numbered()?;
        let size = TerminalSize { cols: self.terminal.cols()?, rows: self.terminal.rows()? };
        let rows = (0..size.rows)
            .map(|y| {
                row(&self.terminal, Place::Screen(y)).and_then(|row| RowJson::new(&row).map_err(MirrorError::from))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let cursor = self.cursor()?;
        let state = state(self)?;
        Ok(Shown { numbering: now.numbering, size, top: now.top, first: now.first, rows, cursor, state })
    }

    /// The row of the shell's history with `number`, in the numbering it's in now, while it keeps it.
    #[must_use]
    pub fn history_row(&self, number: RowNumber) -> Option<RowJson> {
        let now = self.rows.as_ref()?.now();
        if !now.shell || number < now.first || number >= now.top {
            return None;
        }
        RowJson::new(&row(&self.terminal, Place::Kept(number.get() - now.first.get())).ok()?).ok()
    }

    /// Whether the program has asked for what it writes to be shown all at once, which it does for a screen it's
    /// redrawing.
    #[must_use]
    pub fn synchronized(&self) -> bool {
        self.terminal.mode(Mode::SYNC_OUTPUT).unwrap_or(false)
    }

    fn cursor(&mut self) -> Result<Option<TerminalCursor>, MirrorError> {
        let picturing = match &mut self.picturing {
            Some(picturing) => picturing,
            None => self.picturing.insert(Picturing { render: RenderState::new()? }),
        };
        let snapshot = picturing.render.update(&self.terminal)?;
        let cursor = match (snapshot.cursor_visible()?, snapshot.cursor_viewport()?) {
            (true, Some(at)) => Some(TerminalCursor {
                col: if at.at_wide_tail { at.x.saturating_sub(1) } else { at.x },
                row: at.y,
                shape: shape(snapshot.cursor_visual_style()?),
                blinking: snapshot.cursor_blinking()?,
            }),
            (true | false, _) => None,
        };
        // Its rows are never read, so there's nothing to mark.
        snapshot.set_dirty(Dirty::Clean)?;
        Ok(cursor)
    }
}

fn state(mirror: &Mirror) -> Result<ScreenState, MirrorError> {
    let terminal = &mirror.terminal;
    let program = terminal.active_screen()? == Screen::Alternate;
    // As `Mirror::wheel` does with a turn of the wheel.
    let wheel = if terminal.is_mouse_tracking()? {
        Wheel::Program
    } else if program && terminal.mode(Mode::ALT_SCROLL)? {
        Wheel::Arrows
    } else {
        Wheel::Scrolls
    };
    let (palette, default) = (terminal.color_palette()?, terminal.default_color_palette()?);
    let palette = (0..=u8::MAX)
        .zip(palette.0.iter().zip(default.0.iter()))
        .filter(|(_, (now, default))| now != default)
        .map(|(index, (now, _))| PaletteColor { index, color: color(*now) })
        .collect();
    Ok(ScreenState {
        program,
        wheel,
        title: String::from_utf8_lossy(terminal.title()?).into_owned(),
        palette,
        foreground: terminal.fg_color()?.map(color),
        background: terminal.bg_color()?.map(color),
        cursor_color: terminal.cursor_color()?.map(color),
        reverse: terminal.mode(Mode::REVERSE_COLORS)?,
    })
}

const fn color(rgb: Rgb) -> CellColor {
    CellColor::Rgb { r: rgb.r, g: rgb.g, b: rgb.b }
}

fn shape(style: CursorVisualStyle) -> CursorShape {
    match style {
        CursorVisualStyle::Bar => CursorShape::Bar,
        CursorVisualStyle::Block => CursorShape::Block,
        CursorVisualStyle::Underline => CursorShape::Underline,
        CursorVisualStyle::BlockHollow => CursorShape::BlockHollow,
        _ => CursorShape::Unknown,
    }
}
