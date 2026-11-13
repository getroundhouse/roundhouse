//! The client's copy of a terminal, kept from what the hub sends of it, and what that has the client send the hub:
//! what's typed, by the modes the program set, and the size the view has room for. What's selected is the copy's alone,
//! as it's this device's, so it goes with a new copy.

use std::sync::Arc;

use protocol::terminal::{TerminalSize, TerminalUpdate};
use terminal_state::drawn::{Frame, Rgb};
use terminal_state::keys::KeyPress;
use terminal_state::{Mirror, MirrorError, Pointer, SelectBy, Size};

#[cfg(test)]
pub(super) mod tests;

/// What the terminal shows, as far as the client has heard.
pub(super) struct TerminalCopy {
    /// None until the first screen comes.
    mirror: Option<Mirror>,
    /// The colours a cell that sets none is drawn in, once the view has said, and Ghostty's until then.
    colors: Option<(Rgb, Rgb)>,
    /// The size the view has room for, once it has been laid out.
    fit: Option<Size>,
    /// The size the copy last asked the terminal to take, until the hub says it has, or a new screen comes. Until then
    /// it's what the terminal will be, so the view's room going back to the size it had asks for that again.
    asked: Option<Size>,
    /// Whether it has changed since the last frame.
    changed: bool,
}

/// What a change to the copy has the client send the hub.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ToHub {
    /// What the program reads, for what was typed.
    pub(super) input: Vec<u8>,
    /// A new size for the terminal.
    pub(super) resize: Option<TerminalSize>,
}

impl TerminalCopy {
    pub(super) fn new() -> Self {
        Self { mirror: None, colors: None, fit: None, asked: None, changed: true }
    }

    /// Draws a cell that sets no colour in `foreground` on `background`, the view's own, from now on.
    pub(super) fn set_colors(&mut self, foreground: Rgb, background: Rgb) -> Result<(), MirrorError> {
        self.colors = Some((foreground, background));
        let Some(mirror) = &mut self.mirror else { return Ok(()) };
        mirror.set_colors(foreground, background)?;
        self.changed = true;
        Ok(())
    }

    /// Takes in what the hub sent. A screen starts a new copy, which asks for the size the view has room for if it
    /// isn't that size already, so a view that attaches, or attaches again, takes the terminal back to its own size. A
    /// new size from elsewhere is kept, though, since the client that gave it is showing it too.
    pub(super) fn take_in(&mut self, update: TerminalUpdate) -> Result<ToHub, MirrorError> {
        let mut to_hub = ToHub::default();
        match update {
            TerminalUpdate::Screen(screen) => {
                let size = size(screen.size);
                let mut mirror = Mirror::new(size)?;
                if let Some((foreground, background)) = self.colors {
                    mirror.set_colors(foreground, background)?;
                }
                mirror.write(screen.bytes.as_slice());
                self.mirror = Some(mirror);
                self.asked = self.fit.filter(|fit| *fit != size);
                to_hub.resize = self.asked.map(terminal_size);
            }
            TerminalUpdate::Output { output } => {
                if let Some(mirror) = &mut self.mirror {
                    mirror.write(output.as_slice());
                }
            }
            TerminalUpdate::Resized { size: resized } => {
                let resized = size(resized);
                if let Some(mirror) = &mut self.mirror {
                    mirror.resize(resized)?;
                }
                if self.asked == Some(resized) {
                    self.asked = None;
                }
            }
            // The open terminal keeps that the shell has ended, and an update from a newer hub changes nothing.
            TerminalUpdate::Exited(_) | TerminalUpdate::Unknown => return Ok(to_hub),
        }
        self.changed = true;
        Ok(to_hub)
    }

    /// The view has room for `fit`, which the terminal is asked to take if it isn't that size already, or about to be.
    pub(super) fn fit(&mut self, fit: Size) -> Result<ToHub, MirrorError> {
        let changed = self.fit != Some(fit);
        self.fit = Some(fit);
        let now = self.mirror.as_ref().map(Mirror::size).transpose()?;
        let will_be = self.asked.or(now);
        if !changed || will_be.is_none_or(|will_be| will_be == fit) {
            return Ok(ToHub::default());
        }
        self.asked = Some(fit);
        Ok(ToHub { resize: Some(terminal_size(fit)), ..ToHub::default() })
    }

    /// A key. One the program reads anything for also goes back to the screen from wherever the history is scrolled to,
    /// and clears what's selected, as typing does in Ghostty. Nothing is sent before the first screen, since what the
    /// program reads for a key depends on the modes it set.
    pub(super) fn key(&mut self, press: &KeyPress) -> Result<ToHub, MirrorError> {
        let Some(mirror) = &mut self.mirror else { return Ok(ToHub::default()) };
        let input = mirror.key(press)?;
        if !input.is_empty() {
            mirror.scroll_to_screen();
            mirror.clear_selection()?;
            self.changed = true;
        }
        Ok(ToHub { input, ..ToHub::default() })
    }

    pub(super) fn paste(&mut self, text: &str) -> Result<ToHub, MirrorError> {
        let Some(mirror) = &mut self.mirror else { return Ok(ToHub::default()) };
        let input = mirror.paste(text)?;
        mirror.scroll_to_screen();
        mirror.clear_selection()?;
        self.changed = true;
        Ok(ToHub { input, ..ToHub::default() })
    }

    /// A press at `at`, which starts a selection, or selects the word or the line there, as `by` says.
    pub(super) fn press(&mut self, at: Pointer, by: SelectBy) -> Result<(), MirrorError> {
        self.select(|mirror| mirror.press(at, by))
    }

    pub(super) fn drag(&mut self, at: Pointer) -> Result<(), MirrorError> {
        self.select(|mirror| mirror.drag(at))
    }

    /// Ghostty's release changes no selection, so nothing has to be drawn again.
    pub(super) fn release(&mut self, at: Pointer) -> Result<(), MirrorError> {
        self.mirror.as_mut().map_or(Ok(()), |mirror| mirror.release(at))
    }

    pub(super) fn select_all(&mut self) -> Result<(), MirrorError> {
        self.select(Mirror::select_all)
    }

    /// What's selected, as text to copy.
    pub(super) fn selected_text(&self) -> Result<Option<String>, MirrorError> {
        self.mirror.as_ref().map_or(Ok(None), Mirror::selected_text)
    }

    fn select(&mut self, change: impl FnOnce(&mut Mirror) -> Result<(), MirrorError>) -> Result<(), MirrorError> {
        let Some(mirror) = &mut self.mirror else { return Ok(()) };
        change(mirror)?;
        self.changed = true;
        Ok(())
    }

    /// Turns the wheel by `lines`, up for fewer than none, which scrolls through the history, or is sent to a
    /// full-screen program.
    pub(super) fn wheel(&mut self, lines: isize) -> Result<ToHub, MirrorError> {
        let Some(mirror) = &mut self.mirror else { return Ok(ToHub::default()) };
        let input = mirror.wheel(lines)?;
        self.changed = true;
        Ok(ToHub { input, ..ToHub::default() })
    }

    /// What it shows, if that has changed since the last frame.
    pub(super) fn frame(&mut self) -> Result<Option<Arc<Frame>>, MirrorError> {
        if !std::mem::take(&mut self.changed) {
            return Ok(None);
        }
        let Some(mirror) = &mut self.mirror else { return Ok(None) };
        let frame = mirror.frame();
        // So the next try copies it again.
        self.changed = frame.is_err();
        Ok(Some(Arc::new(frame?)))
    }
}

fn size(size: TerminalSize) -> Size {
    Size { cols: size.cols, rows: size.rows }
}

fn terminal_size(size: Size) -> TerminalSize {
    TerminalSize { cols: size.cols, rows: size.rows }
}
