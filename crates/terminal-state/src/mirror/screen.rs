use libghostty_vt::Terminal;
use libghostty_vt::effect::{
    ConformanceLevel, DeviceAttributeFeature, DeviceAttributes, DeviceType, PrimaryDeviceAttributes,
    SecondaryDeviceAttributes, TertiaryDeviceAttributes,
};
use protocol::terminal::TerminalSize;

use super::compressing::Compressing;

/// Lines kept above the screen, as most terminals keep by default.
pub(super) const HISTORY_LINES: usize = 10_000;
/// The most the lines kept above the screen take, which [`HISTORY_LINES`] fill at the widths people use, so it stops
/// only a wide or colourful terminal short of them. Without it, Ghostty's own limit of about one page, 400 KiB, would
/// keep about a thousand lines at 80 columns and two hundred at 200. The worker's copy holds the only history. At
/// `TerminalSize::MOST`, 600 columns, it would otherwise take about 50 MiB, and this keeps about 6,000 lines.
pub(super) const HISTORY_BYTES: usize = 32 * 1024 * 1024;
/// What the terminal says it is when a program asks, which is what most terminals say: a VT220, with colour.
const ATTRIBUTES: DeviceAttributes = DeviceAttributes {
    primary: PrimaryDeviceAttributes::new(ConformanceLevel::VT220, &[DeviceAttributeFeature::ANSI_COLOR]),
    secondary: SecondaryDeviceAttributes { device_type: DeviceType::VT220, firmware_version: 0, rom_cartridge: 0 },
    tertiary: TertiaryDeviceAttributes { unit_id: 0 },
};

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    /// Ghostty refused, such as for a size of zero.
    #[error(transparent)]
    Terminal(#[from] libghostty_vt::Error),
    /// A row couldn't be written as JSON, which none that Ghostty holds fails.
    #[error("couldn't write a row: {0}")]
    Row(#[from] serde_json::Error),
}

/// What a terminal shows, kept from everything the program in it writes. It isn't `Send`, since Ghostty's terminal
/// isn't, so it lives on the thread that reads its terminal's output.
#[derive(Debug)]
pub struct Mirror {
    pub(super) terminal: Terminal<'static, 'static>,
    pub(super) typing: Option<super::typing::Typing>,
    pub(super) compressing: Compressing,
    /// The count of its rows, once they've been asked for by number.
    pub(super) rows: Option<super::rows::Counting>,
    /// What it reads to say how its cursor is drawn, once it's asked for a picture.
    pub(super) picturing: Option<super::rows::Picturing>,
}

impl Mirror {
    /// A copy that answers nothing the program asks, as a test's is, such as the fake hub's.
    pub fn new(size: TerminalSize) -> Result<Self, MirrorError> {
        let mut terminal = Terminal::new(size.cols, size.rows)?;
        terminal.set_scrollback_max_lines(Some(HISTORY_LINES))?.set_scrollback_max_bytes(Some(HISTORY_BYTES))?;
        // A picture is rows of cells, with no images, so the kitty graphics protocol is off. A program that asks
        // whether it can show images is then told nothing, and shows text instead, and none it sends are kept.
        terminal.set_kitty_image_storage_limit(0)?;
        let compressing = Compressing::default();
        Ok(Self { terminal, typing: None, compressing, rows: None, picturing: None })
    }

    /// A copy that answers what the program asks of its terminal, such as where the cursor is, or what the terminal
    /// is, by giving `answer` what the program reads back. It's called as a write or a new size takes in a question.
    /// Only the worker's copy answers, so each question is answered once, whether several clients watch or none, and
    /// a shell that asks as it starts doesn't wait for a client.
    pub fn answering(size: TerminalSize, mut answer: impl FnMut(&[u8]) + 'static) -> Result<Self, MirrorError> {
        let mut mirror = Self::new(size)?;
        mirror.terminal.on_pty_write(move |_, bytes| answer(bytes))?.on_device_attributes(|_| Some(ATTRIBUTES))?;
        Ok(mirror)
    }

    /// What the program wrote, however its output was split up, such as partway through a sequence.
    pub fn write(&mut self, output: &[u8]) {
        self.terminal.vt_write(output);
        if let Some(counting) = &mut self.rows
            && counting.written(&mut self.terminal).is_err()
        {
            // Ghostty fails only when it can't allocate.
            counting.lost();
        }
    }

    /// Gives the terminal a new size, which rewraps the shell's screen and its history, as the program's terminal has,
    /// and starts its rows' numbering again. The size it has already changes nothing.
    pub fn resize(&mut self, size: TerminalSize) -> Result<(), MirrorError> {
        if self.size()? == size {
            return Ok(());
        }
        // Nothing here knows how big a cell is, so a program that asks the size in pixels is told nothing.
        let resized = self.terminal.resize(size.cols, size.rows, 0, 0).map_err(MirrorError::from);
        let Some(counting) = &mut self.rows else { return resized };
        let counted = resized.and_then(|()| counting.resized(&mut self.terminal));
        if counted.is_err() {
            counting.lost();
        }
        counted
    }

    pub(in crate::mirror) fn size(&self) -> Result<TerminalSize, MirrorError> {
        Ok(TerminalSize { cols: self.terminal.cols()?, rows: self.terminal.rows()? })
    }
}
