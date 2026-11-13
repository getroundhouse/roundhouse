//! A key as a view hears it, for a mirror to turn into what the program in the terminal reads, by the modes the
//! program has set, such as the cursor keys' application mode.

/// One press of a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    pub mods: Mods,
    /// What the key typed, with its modifiers, such as `A` with shift, or `å` with option on a Mac.
    pub text: Option<String>,
    /// The modifiers that went into `text`, such as shift for `A`, so they aren't sent as well.
    pub consumed: Mods,
}

/// Which key, by what it is on a US keyboard, so a program that reads keys rather than text, such as one using the
/// kitty protocol, is told the same key on every layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A key that types a character, by the one it types with no modifiers, such as `a` or `;`.
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// A function key, from 1 to 12.
    F(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each is a key of its own, held or not")]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Command on a Mac, and the Windows key elsewhere.
    pub super_key: bool,
}
