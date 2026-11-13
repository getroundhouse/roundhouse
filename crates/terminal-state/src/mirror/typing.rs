//! What the program in a terminal reads for a key, a paste or a turn of the wheel, by the modes it has set, which only
//! a copy of what it wrote knows.

use libghostty_vt::key::{self, Action, Encoder, Event};
use libghostty_vt::mode::Mode;
use libghostty_vt::screen::Screen;
use libghostty_vt::terminal::ScrollViewport;

use super::{Mirror, MirrorError};
use crate::keys::{Key, KeyPress, Mods};

#[cfg(test)]
mod tests;

/// What a mirror keeps to encode keys, made the first time it encodes one, since the worker's never does.
pub(super) struct Typing {
    encoder: Encoder<'static>,
    event: Event<'static>,
}

impl std::fmt::Debug for Typing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typing").finish_non_exhaustive()
    }
}

impl Mirror {
    /// What the program reads for the key, which is nothing for one it has no sequence for, such as a function key past
    /// those it knows.
    pub fn key(&mut self, press: &KeyPress) -> Result<Vec<u8>, MirrorError> {
        let typing = match &mut self.typing {
            Some(typing) => typing,
            None => self.typing.insert(Typing { encoder: Encoder::new()?, event: Event::new()? }),
        };
        let (key, unshifted) = ghostty_key(press.key);
        typing
            .event
            .set_action(Action::Press)
            .set_key(key)
            .set_mods(ghostty_mods(press.mods))
            .set_consumed_mods(ghostty_mods(press.consumed))
            .set_unshifted_codepoint(unshifted)
            .set_utf8(press.text.clone());
        let mut out = Vec::new();
        typing.encoder.set_options_from_terminal(&self.terminal).encode_to_vec(&typing.event, &mut out)?;
        Ok(out)
    }

    /// What the program reads for pasted text: the text with Escape, Backspace, Delete and the control characters a
    /// terminal acts on, such as Control-C, swapped for spaces, as xterm does, so it can't end its own marks early or
    /// stop the program, between the marks that say it's pasted if the program asked for them. Otherwise each line
    /// ends as Return does, so a shell runs each line of it.
    pub fn paste(&self, text: &str) -> Result<Vec<u8>, MirrorError> {
        let bracketed = self.terminal.mode(Mode::BRACKETED_PASTE)?;
        let mut text = text.as_bytes().to_vec();
        // The marks and the text, each byte of which is kept or swapped for one other.
        let mut out = vec![0; text.len() + 12];
        let written = libghostty_vt::paste::encode(&mut text, bracketed, &mut out)?;
        out.truncate(written);
        Ok(out)
    }

    /// Turns the wheel by `lines`, up for fewer than none. On the shell's screen it scrolls through the history. A
    /// full-screen program's screen has none, so as terminal apps do, it's sent as the arrow keys instead, which is
    /// what's returned, for the program to scroll itself. A program that asked for the mouse is sent nothing, until the
    /// mouse is sent to programs, since it would take arrow keys for keys typed, as tmux does, which brings back old
    /// commands for a Return to run.
    pub fn wheel(&mut self, lines: isize) -> Result<Vec<u8>, MirrorError> {
        if self.terminal.is_mouse_tracking()? {
            return Ok(Vec::new());
        }
        if self.terminal.active_screen()? == Screen::Alternate && self.terminal.mode(Mode::ALT_SCROLL)? {
            let arrow = KeyPress {
                key: if lines < 0 { Key::Up } else { Key::Down },
                mods: Mods::default(),
                text: None,
                consumed: Mods::default(),
            };
            return Ok(self.key(&arrow)?.repeat(lines.unsigned_abs()));
        }
        self.terminal.scroll_viewport(ScrollViewport::Delta(lines));
        Ok(Vec::new())
    }

    /// Goes back to the screen from wherever the history is scrolled to, as typing does.
    pub fn scroll_to_screen(&mut self) {
        self.terminal.scroll_viewport(ScrollViewport::Bottom);
    }
}

fn ghostty_mods(mods: Mods) -> key::Mods {
    let mut out = key::Mods::empty();
    out.set(key::Mods::SHIFT, mods.shift);
    out.set(key::Mods::CTRL, mods.ctrl);
    out.set(key::Mods::ALT, mods.alt);
    out.set(key::Mods::SUPER, mods.super_key);
    out
}

/// Ghostty's key for `key`, and the character it types unshifted, which is none for a key that types none.
fn ghostty_key(key: Key) -> (key::Key, char) {
    let named = match key {
        Key::Char(typed) => return char_key(typed),
        Key::Enter => key::Key::Enter,
        Key::Tab => key::Key::Tab,
        Key::Backspace => key::Key::Backspace,
        Key::Escape => key::Key::Escape,
        Key::Space => return (key::Key::Space, ' '),
        Key::Up => key::Key::ArrowUp,
        Key::Down => key::Key::ArrowDown,
        Key::Left => key::Key::ArrowLeft,
        Key::Right => key::Key::ArrowRight,
        Key::Home => key::Key::Home,
        Key::End => key::Key::End,
        Key::PageUp => key::Key::PageUp,
        Key::PageDown => key::Key::PageDown,
        Key::Insert => key::Key::Insert,
        Key::Delete => key::Key::Delete,
        Key::F(number) => function_key(number),
    };
    (named, '\0')
}

const LETTERS: [key::Key; 26] = [
    key::Key::A,
    key::Key::B,
    key::Key::C,
    key::Key::D,
    key::Key::E,
    key::Key::F,
    key::Key::G,
    key::Key::H,
    key::Key::I,
    key::Key::J,
    key::Key::K,
    key::Key::L,
    key::Key::M,
    key::Key::N,
    key::Key::O,
    key::Key::P,
    key::Key::Q,
    key::Key::R,
    key::Key::S,
    key::Key::T,
    key::Key::U,
    key::Key::V,
    key::Key::W,
    key::Key::X,
    key::Key::Y,
    key::Key::Z,
];

const DIGITS: [key::Key; 10] = [
    key::Key::Digit0,
    key::Key::Digit1,
    key::Key::Digit2,
    key::Key::Digit3,
    key::Key::Digit4,
    key::Key::Digit5,
    key::Key::Digit6,
    key::Key::Digit7,
    key::Key::Digit8,
    key::Key::Digit9,
];

const FUNCTION_KEYS: [key::Key; 12] = [
    key::Key::F1,
    key::Key::F2,
    key::Key::F3,
    key::Key::F4,
    key::Key::F5,
    key::Key::F6,
    key::Key::F7,
    key::Key::F8,
    key::Key::F9,
    key::Key::F10,
    key::Key::F11,
    key::Key::F12,
];

/// The key that types `typed` on a US keyboard, or none for a character it has no key for, which is then sent as
/// text alone.
fn char_key(typed: char) -> (key::Key, char) {
    let lower = typed.to_ascii_lowercase();
    let found = match lower {
        'a'..='z' => LETTERS[usize::from(lower as u8 - b'a')],
        '0'..='9' => DIGITS[usize::from(lower as u8 - b'0')],
        '`' => key::Key::Backquote,
        '\\' => key::Key::Backslash,
        '[' => key::Key::BracketLeft,
        ']' => key::Key::BracketRight,
        ',' => key::Key::Comma,
        '=' => key::Key::Equal,
        '-' => key::Key::Minus,
        '.' => key::Key::Period,
        '\'' => key::Key::Quote,
        ';' => key::Key::Semicolon,
        '/' => key::Key::Slash,
        ' ' => key::Key::Space,
        _ => key::Key::Unidentified,
    };
    (found, lower)
}

fn function_key(number: u8) -> key::Key {
    usize::from(number)
        .checked_sub(1)
        .and_then(|index| FUNCTION_KEYS.get(index))
        .copied()
        .unwrap_or(key::Key::Unidentified)
}
