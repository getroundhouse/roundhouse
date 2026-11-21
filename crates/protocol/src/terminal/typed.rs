//! What's pressed in a terminal, as a client hears it, for the worker to turn into what the program reads by the modes
//! it has set, such as the cursor keys' application mode, which only the worker's copy of the terminal knows.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One thing pressed. What's typed is kept out of `Debug`, since it may be a password.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Typed {
    Key(KeyPress),
    /// Text pasted, which the program reads between the marks that say it's pasted if it asked for them.
    Paste {
        text: String,
    },
    /// A turn of the wheel by `lines`, up for fewer than none, which the program reads as arrow keys on a full-screen
    /// program's screen that asked for them, and otherwise as nothing, since the client scrolls the history itself.
    Wheel {
        lines: i32,
    },
    /// Text as it is, such as what an input method finished composing.
    Text {
        text: String,
    },
    /// One a newer peer knows, which the program is sent nothing for.
    #[serde(other)]
    Unknown,
}

impl std::fmt::Debug for Typed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(KeyPress { key: Key::Char(_), mods, .. }) => write!(f, "Key(a character, {mods:?})"),
            Self::Key(KeyPress { key, mods, .. }) => write!(f, "Key({key:?}, {mods:?})"),
            Self::Paste { text } => write!(f, "Paste({} bytes)", text.len()),
            Self::Wheel { lines } => write!(f, "Wheel({lines})"),
            Self::Text { text } => write!(f, "Text({} bytes)", text.len()),
            Self::Unknown => f.write_str("Unknown"),
        }
    }
}

/// One press of a key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    #[serde(default, skip_serializing_if = "Mods::is_none")]
    pub mods: Mods,
    /// What the key typed, with its modifiers, such as `A` with shift, or `å` with option on a Mac.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The modifiers that went into `text`, such as shift for `A`, so they aren't sent as well.
    #[serde(default, skip_serializing_if = "Mods::is_none")]
    pub consumed: Mods,
}

/// Which key, by what it is on a US keyboard, so a program that reads keys rather than text, such as one using the
/// kitty protocol, is told the same key on every layout. Written as one string: the character, for a key that types
/// one, and otherwise its name, such as `enter` or `f5`.
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
    /// One a newer peer knows, which the program is sent only the text of.
    Unknown,
}

const NAMED: [(Key, &str); 15] = [
    (Key::Enter, "enter"),
    (Key::Tab, "tab"),
    (Key::Backspace, "backspace"),
    (Key::Escape, "escape"),
    (Key::Space, "space"),
    (Key::Up, "up"),
    (Key::Down, "down"),
    (Key::Left, "left"),
    (Key::Right, "right"),
    (Key::Home, "home"),
    (Key::End, "end"),
    (Key::PageUp, "page_up"),
    (Key::PageDown, "page_down"),
    (Key::Insert, "insert"),
    (Key::Delete, "delete"),
];

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Self::Char(typed) = self {
            return serializer.collect_str(typed);
        }
        if let Self::F(number) = self {
            return serializer.collect_str(&format_args!("f{number}"));
        }
        let name = NAMED.iter().find(|(key, _)| key == self).map_or("unknown", |(_, name)| name);
        serializer.serialize_str(name)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let written = String::deserialize(deserializer)?;
        let mut chars = written.chars();
        if let (Some(typed), None) = (chars.next(), chars.next()) {
            return Ok(Self::Char(typed));
        }
        if let Some((key, _)) = NAMED.iter().find(|(_, name)| *name == written) {
            return Ok(*key);
        }
        let function =
            written.strip_prefix('f').and_then(|number| number.parse().ok()).filter(|n| (1..=12).contains(n));
        Ok(function.map_or(Self::Unknown, Self::F))
    }
}

/// The modifiers held, each left out of what's written while it isn't.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each is a key of its own, held or not")]
pub struct Mods {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alt: bool,
    /// Command on a Mac, and the Windows key elsewhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not", rename = "super")]
    pub super_key: bool,
}

impl Mods {
    #[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
    fn is_none(&self) -> bool {
        *self == Self::default()
    }
}
