//! A key GPUI heard, as a key for the terminal, unless it's one the app keeps for itself.

use gpui_kit::{KeyDownEvent, OperatingSystem};
use terminal_state::keys::{Key, KeyPress, Mods};

/// The key for the terminal, or none for one that isn't the terminal's: Command with anything on a Mac, which is the
/// app's, as in other Mac terminal apps, but for the keys `bind_keys` gives the meaning Ghostty does, which never come
/// here, a modifier pressed alone, and a key the terminal has no use for.
pub(crate) fn key_press(event: &KeyDownEvent, os: OperatingSystem) -> Option<KeyPress> {
    let (keystroke, mac) = (&event.keystroke, os == OperatingSystem::Mac);
    let held = &keystroke.modifiers;
    if mac && held.platform {
        return None;
    }
    let key = match keystroke.key.as_str() {
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "escape" => Key::Escape,
        "space" => Key::Space,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        name => named_or_char(name)?,
    };
    // A control character isn't what was typed, such as the `\n` a Mac gives Return, so the key is read with every
    // modifier held, as Shift-Return is by a program that reads keys as keys.
    let typed = keystroke.key_char.clone().filter(|typed| !typed.is_empty() && !typed.chars().any(char::is_control));
    // With Control, the key is what's sent, as Control and the key, rather than what the platform says it typed, unless
    // GPUI says the modifiers went into typing it, as AltGr's do on Windows, which it says are Control and Alt.
    let text = typed.filter(|_| !held.control || event.prefer_character_input);
    // Then the program reads only the character, as other apps there do, since Control with it would be read as a
    // control character.
    let typed_with_modifiers = event.prefer_character_input && text.is_some();
    let (ctrl, alt) = (held.control && !typed_with_modifiers, held.alt && !typed_with_modifiers);
    let mods = Mods { shift: held.shift, ctrl, alt, super_key: held.platform };
    // Shift went into what was typed, and so did Option on a Mac, which types a character, as it does in other apps.
    let consumed = Mods { shift: held.shift && text.is_some(), alt: mac && alt && text.is_some(), ..Mods::default() };
    Some(KeyPress { key, mods, text, consumed })
}

/// A function key by its name, such as `f5`, or a key that types one character.
fn named_or_char(name: &str) -> Option<Key> {
    if let Some(number) = name.strip_prefix('f').and_then(|number| number.parse::<u8>().ok()) {
        return (1..=12).contains(&number).then_some(Key::F(number));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => Some(Key::Char(only)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{KeyDownEvent, Keystroke, Modifiers, OperatingSystem};
    use terminal_state::keys::{Key, KeyPress, Mods};

    use super::key_press;

    const MAC: OperatingSystem = OperatingSystem::Mac;
    const LINUX: OperatingSystem = OperatingSystem::Linux;
    const WINDOWS: OperatingSystem = OperatingSystem::Windows;

    fn stroke(key: &str, typed: Option<&str>, modifiers: Modifiers) -> KeyDownEvent {
        let keystroke = Keystroke { modifiers, key: key.to_owned(), key_char: typed.map(str::to_owned) };
        KeyDownEvent { keystroke, is_held: false, prefer_character_input: false }
    }

    #[test]
    fn a_key_that_types_is_sent_with_what_it_typed() {
        let shifted = Modifiers { shift: true, ..Modifiers::default() };

        let press = key_press(&stroke("a", Some("A"), shifted), MAC).unwrap();

        let shift = Mods { shift: true, ..Mods::default() };
        assert_eq!(press, KeyPress { key: Key::Char('a'), mods: shift, text: Some("A".into()), consumed: shift });
    }

    #[test]
    fn control_sends_the_key_rather_than_what_the_platform_typed() {
        let control = Modifiers { control: true, ..Modifiers::default() };

        let press = key_press(&stroke("c", Some("\u{3}"), control), MAC).unwrap();

        assert_eq!((press.key, press.text), (Key::Char('c'), None));
        assert!(press.mods.ctrl);
    }

    /// On a Mac, Command is the app's, so its shortcuts work from a terminal. Elsewhere it's Super, which goes to the
    /// terminal, but for the keys the app binds, which take it first.
    #[test]
    fn command_is_the_apps_on_a_mac() {
        let command = Modifiers { platform: true, ..Modifiers::default() };

        assert_eq!(key_press(&stroke("w", None, command), MAC), None);
        assert!(key_press(&stroke("w", None, command), LINUX).is_some_and(|press| press.mods.super_key));
    }

    #[test]
    fn option_types_a_character_on_a_mac() {
        let option = Modifiers { alt: true, ..Modifiers::default() };

        let press = key_press(&stroke("s", Some("ß"), option), MAC).unwrap();

        assert_eq!(press.text.as_deref(), Some("ß"));
        assert!(press.consumed.alt);
        assert!(!key_press(&stroke("s", Some("s"), option), LINUX).unwrap().consumed.alt);
    }

    /// GPUI gives Return the text `"\n"` on a Mac, and none elsewhere, and either way the program reads Return.
    #[test]
    fn return_is_read_as_a_carriage_return_whatever_text_it_comes_with() {
        let mut mirror = terminal_state::Mirror::new(terminal_state::Size { cols: 20, rows: 4 }).unwrap();

        for typed in [Some("\n"), None] {
            let press = key_press(&stroke("enter", typed, Modifiers::default()), MAC).unwrap();
            assert_eq!(mirror.key(&press).unwrap(), b"\r", "{typed:?}");
        }
    }

    /// A Mac gives Return the text `"\n"`, which isn't what Shift typed, so Shift stays held for a program that reads
    /// keys as keys, as Claude Code reads Shift-Return.
    #[test]
    fn shift_return_reaches_a_program_that_reads_keys_as_keys() {
        let mut mirror = terminal_state::Mirror::new(terminal_state::Size { cols: 20, rows: 4 }).unwrap();
        mirror.write(b"\x1b[>1u");
        let shifted = Modifiers { shift: true, ..Modifiers::default() };

        for typed in [Some("\n"), None] {
            let press = key_press(&stroke("enter", typed, shifted), MAC).unwrap();
            assert_eq!(mirror.key(&press).unwrap(), b"\x1b[13;2u", "{typed:?}");
        }
    }

    /// On a Mac, Option with a key that types no character, such as Return or Backspace, is sent as Alt with the key,
    /// since it didn't go into typing anything.
    #[test]
    fn option_with_a_key_that_types_nothing_is_sent_as_alt_on_a_mac() {
        let mut mirror = terminal_state::Mirror::new(terminal_state::Size { cols: 20, rows: 4 }).unwrap();
        let option = Modifiers { alt: true, ..Modifiers::default() };

        for typed in [Some("\n"), None] {
            let press = key_press(&stroke("enter", typed, option), MAC).unwrap();
            assert_eq!(mirror.key(&press).unwrap(), b"\x1b\r", "{typed:?}");
        }
        let press = key_press(&stroke("backspace", None, option), MAC).unwrap();
        assert_eq!(mirror.key(&press).unwrap(), b"\x1b\x7f");
    }

    /// Windows says `AltGr` is Control and Alt held, which went into typing the character, such as `@` on a German
    /// keyboard, so it's typed. Control and Alt that didn't are sent with the key.
    #[test]
    fn a_character_typed_with_altgr_is_typed() {
        let mut mirror = terminal_state::Mirror::new(terminal_state::Size { cols: 20, rows: 4 }).unwrap();
        let altgr = Modifiers { control: true, alt: true, ..Modifiers::default() };

        let altgr_q = KeyDownEvent { prefer_character_input: true, ..stroke("q", Some("@"), altgr) };

        let typed = key_press(&altgr_q, WINDOWS).unwrap();
        let chord = key_press(&stroke("q", None, altgr), WINDOWS).unwrap();

        assert_eq!(mirror.key(&typed).unwrap(), b"@");
        assert_eq!(mirror.key(&chord).unwrap(), b"\x1b\x11");
    }

    #[test]
    fn named_keys_and_function_keys_are_the_terminals() {
        let none = Modifiers::default();

        assert_eq!(key_press(&stroke("pageup", None, none), MAC).map(|press| press.key), Some(Key::PageUp));
        assert_eq!(key_press(&stroke("f12", None, none), MAC).map(|press| press.key), Some(Key::F(12)));
        assert_eq!(key_press(&stroke("f13", None, none), MAC), None);
        assert_eq!(key_press(&stroke("shift", None, none), MAC), None, "a modifier alone");
    }
}
