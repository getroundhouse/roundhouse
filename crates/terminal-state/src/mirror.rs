//! A copy of one terminal, the worker's or a client's. Everything the program in it writes goes through it, and it
//! gives back what a fresh terminal needs to show the same, what a view draws, and what the program reads for a key.

use libghostty_vt::Terminal;
use libghostty_vt::effect::{
    ConformanceLevel, DeviceAttributeFeature, DeviceAttributes, DeviceType, PrimaryDeviceAttributes,
    SecondaryDeviceAttributes, TertiaryDeviceAttributes,
};
use libghostty_vt::fmt::{Extras, Format, Formatter};
use libghostty_vt::screen::Screen;
use libghostty_vt::snapshot::Decoder;

use crate::drawn::Rgb;

mod drawing;
mod selecting;
#[cfg(test)]
mod tests;
mod typing;

pub use selecting::{Pointer, SelectBy};

/// Abandons any sequence a copy was partway through, then goes back to the shell's screen, as vim does as it quits.
const TO_SHELL_SCREEN: &[u8] = b"\x18\x1b[?1049l";
/// Lines kept above the screen, as most terminals keep by default.
const HISTORY_LINES: usize = 10_000;
/// The longest unfinished sequence a snapshot carries over, which covers everything but a large image or clipboard
/// write cut off partway.
const LONGEST_UNFINISHED_BYTES: usize = 64 * 1024;
/// What the terminal says it is when a program asks, which is what most terminals say: a VT220, with colour.
const ATTRIBUTES: DeviceAttributes = DeviceAttributes {
    primary: PrimaryDeviceAttributes::new(ConformanceLevel::VT220, &[DeviceAttributeFeature::ANSI_COLOR]),
    secondary: SecondaryDeviceAttributes { device_type: DeviceType::VT220, firmware_version: 0, rom_cartridge: 0 },
    tertiary: TertiaryDeviceAttributes { unit_id: 0 },
};

/// How many columns and rows a terminal has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    /// Ghostty refused, such as for a size of zero, or because an unfinished sequence was longer than a snapshot can
    /// carry over.
    #[error(transparent)]
    Terminal(#[from] libghostty_vt::Error),
}

/// What a terminal shows, kept from everything the program in it writes. It isn't `Send`, since Ghostty's terminal
/// isn't, so it lives on the thread that reads its terminal's output.
#[derive(Debug)]
pub struct Mirror {
    terminal: Terminal<'static, 'static>,
    drawing: Option<drawing::Drawing>,
    typing: Option<typing::Typing>,
    selecting: Option<selecting::Selecting>,
}

impl Mirror {
    /// A copy that answers nothing the program asks, as a client's is, since the worker's answers.
    pub fn new(size: Size) -> Result<Self, MirrorError> {
        let mut terminal = Terminal::new(size.cols, size.rows)?;
        terminal.set_scrollback_max_lines(Some(HISTORY_LINES))?;
        // From the start, since only bytes written after it's on can be carried over by a snapshot.
        terminal.set_continuation_max_bytes(LONGEST_UNFINISHED_BYTES)?;
        Ok(Self { terminal, drawing: None, typing: None, selecting: None })
    }

    /// A copy that answers what the program asks of its terminal, such as where the cursor is, or what the terminal
    /// is, by giving `answer` what the program reads back. It's called as a write or a new size takes in a question.
    pub fn answering(size: Size, mut answer: impl FnMut(&[u8]) + 'static) -> Result<Self, MirrorError> {
        let mut mirror = Self::new(size)?;
        mirror.terminal.on_pty_write(move |_, bytes| answer(bytes))?.on_device_attributes(|_| Some(ATTRIBUTES))?;
        Ok(mirror)
    }

    /// What the program wrote, however its output was split up, such as partway through a sequence.
    pub fn write(&mut self, output: &[u8]) {
        self.terminal.vt_write(output);
    }

    /// Gives the terminal a new size, which rewraps the shell's screen and its history, as the program's terminal has.
    pub fn resize(&mut self, size: Size) -> Result<(), MirrorError> {
        // Nothing here knows how big a cell is, so a program that asks the size in pixels is told nothing.
        Ok(self.terminal.resize(size.cols, size.rows, 0, 0)?)
    }

    pub fn size(&self) -> Result<Size, MirrorError> {
        Ok(Size { cols: self.terminal.cols()?, rows: self.terminal.rows()? })
    }

    /// The colours a cell that sets none is drawn in, which a client takes from its own look.
    pub fn set_colors(&mut self, foreground: Rgb, background: Rgb) -> Result<(), MirrorError> {
        let ghostty = |color: Rgb| libghostty_vt::color::Rgb { r: color.r, g: color.g, b: color.b };
        self.terminal
            .set_default_fg_color(Some(ghostty(foreground)))?
            .set_default_bg_color(Some(ghostty(background)))?;
        Ok(())
    }

    /// Bytes that make a fresh terminal of the same size show what this one does, so that what the program writes
    /// next shows there as it does here. They hold the title, the shell's screen and its history, with their colours,
    /// the cursor and the modes, and while a full-screen program is up, its screen too, so quitting it brings back what
    /// was under it. They end with the part of a sequence the program was partway through, so the rest of it lands.
    ///
    /// They're escape sequences rather than Ghostty's own snapshot, whose bytes another build needn't read, since the
    /// worker and the app can be different builds. They leave out where the shell's prompts are, which a client doesn't
    /// need, and a cursor position the program saved on the screen showing, which Ghostty's formatter doesn't write, so
    /// a client that attaches between a save and its restore puts the cursor back at the top left.
    pub fn snapshot(&self) -> Result<Vec<u8>, MirrorError> {
        let mut out = title(&self.terminal)?;
        if self.terminal.active_screen()? == Screen::Alternate {
            out.extend(under_the_program(&self.terminal)?);
        }
        // Among the modes it restores is the switch to the program's screen, so a client enters it once, as the worker
        // did. Ghostty clears a program's screen in the current background colour when it's entered again, so a second
        // switch would paint a screen the worker has blank.
        out.extend(screen(&self.terminal)?);
        if let Some(unfinished) = self.terminal.continuation_alloc(None)? {
            out.extend_from_slice(&unfinished);
        }
        Ok(out)
    }
}

/// The title the program last set, as the sequence that sets it, which the formatter leaves out.
fn title(terminal: &Terminal<'_, '_>) -> Result<Vec<u8>, MirrorError> {
    let title = terminal.title()?;
    if title.is_empty() {
        return Ok(Vec::new());
    }
    Ok([b"\x1b]2;", title, b"\x07"].concat())
}

/// The screen showing, with its history, and the colours, cursor and modes a program left set.
fn screen(terminal: &Terminal<'_, '_>) -> Result<Vec<u8>, MirrorError> {
    let mut formatter = Formatter::new(terminal)?;
    formatter.set_format(Format::Vt)?.set_extras(Extras::ALL)?.set_trim(false)?;
    Ok(formatter.format_alloc(None)?.to_vec())
}

/// The shell's screen under a full-screen program. The formatter writes only the screen showing, so it comes from a
/// copy that has left the program's screen. Ghostty's own snapshot makes the copy, since a build always reads its own.
fn under_the_program(terminal: &Terminal<'_, '_>) -> Result<Vec<u8>, MirrorError> {
    // Nothing to copy is a terminal nothing has been written to, which has nothing under its program either.
    let Some(bytes) = terminal.encode_snapshot_alloc(None)? else {
        return Ok(Vec::new());
    };
    let mut copy = Decoder::new_buf(&bytes)?.decode()?;
    copy.vt_write(TO_SHELL_SCREEN);
    screen(&copy)
}
