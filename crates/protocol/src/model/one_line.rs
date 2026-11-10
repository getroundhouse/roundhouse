//! Text as it reads on one line, for a title or a prompt's line in a list, so a row shows it as it reads.

#[cfg(test)]
pub(super) mod laws;

/// `text` as it reads on one line, at most `most` characters long: each run of whitespace, line breaks included, is one
/// space, with none at either end, and control characters and those [`hides`] are gone. It stops at the limit rather
/// than reading the rest, so a long text costs no more than its start.
pub(super) fn one_line(text: &str, most: usize) -> String {
    let mut line = String::new();
    let (mut chars, mut gap) = (0, false);
    for c in text.chars() {
        if c.is_whitespace() {
            gap = chars > 0;
            continue;
        }
        if c.is_control() || hides(c) {
            continue;
        }
        let needs = if gap { 2 } else { 1 };
        if chars + needs > most {
            break;
        }
        if gap {
            line.push(' ');
            gap = false;
        }
        line.push(c);
        chars += needs;
    }
    line
}

/// Characters that show as nothing or reorder what follows them: bidirectional controls, which can make a row read
/// backwards, and zero-width spaces, which can make a title look empty. Joiners stay, since emoji and some scripts
/// need them.
pub(super) fn hides(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200B}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}
