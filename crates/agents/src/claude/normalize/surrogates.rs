//! JavaScript strings can hold half of a surrogate pair, which `JSON.stringify` writes as an escape that JSON parsers
//! refuse.

/// Replaces `\\uD800`-style escapes that aren't half of a proper pair with U+FFFD.
pub(super) fn repair(line: &str) -> String {
    fn escape_at(bytes: &[u8], at: usize) -> Option<u16> {
        let hex = bytes.get(at + 2..at + 6)?;
        if bytes[at] != b'\\' || bytes[at + 1] != b'u' {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
    }
    const REPLACEMENT: &str = "\\ufffd";
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut copied_to = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let Some(unit) = escape_at(bytes, i) else {
            // Some other escape, such as an escaped backslash. Its second byte isn't an escape start.
            i += 2;
            continue;
        };
        match unit {
            0xD800..=0xDBFF if matches!(escape_at(bytes, i + 6), Some(0xDC00..=0xDFFF)) => i += 12,
            0xD800..=0xDFFF => {
                out.push_str(&line[copied_to..i]);
                out.push_str(REPLACEMENT);
                i += 6;
                copied_to = i;
            }
            _ => i += 6,
        }
    }
    out.push_str(&line[copied_to..]);
    out
}
