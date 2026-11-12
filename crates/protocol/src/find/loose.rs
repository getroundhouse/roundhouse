//! Looking past what a chat doesn't show as it's written: what Markdown marks text with, how much whitespace there is,
//! and how JSON writes a string. A needle and a text are each read as a run of characters, in lower case, without
//! [`MARKS`], and with each run of whitespace as one space, and the needle's run is looked for in the text's.

use super::MARKS;

/// How a text is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Written {
    /// As it is.
    Plain,
    /// As JSON writes a string, with each escape read as what it stands for.
    Json,
}

/// A needle as looking past marks reads it, with where a partial match carries on from when the next character isn't
/// the one wanted, as Knuth, Morris and Pratt's search has it, so a text is read once, a character at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Loose {
    chars: Vec<char>,
    /// For a match of the first `n + 1` characters, how many of them a match can carry on with when the next fails.
    fallback: Vec<usize>,
}

impl Loose {
    pub(super) fn of(query: &str) -> Self {
        let chars: Vec<char> = loosely(query, Written::Plain).collect();
        let mut fallback = vec![0; chars.len()];
        let mut matched = 0;
        for at in 1..chars.len() {
            while matched > 0 && chars[at] != chars[matched] {
                matched = fallback[matched - 1];
            }
            if chars[at] == chars[matched] {
                matched += 1;
            }
            fallback[at] = matched;
        }
        Self { chars, fallback }
    }

    /// Whether there's nothing left of the needle, as of one that's nothing but marks.
    pub(super) fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub(super) fn has_space_at_an_end(&self) -> bool {
        self.chars.first() == Some(&' ') || self.chars.last() == Some(&' ')
    }

    /// Whether `text`, written as `written` says, holds the needle. Never, for a needle with nothing left.
    pub(super) fn is_in(&self, text: &str, written: Written) -> bool {
        let Some(last) = self.chars.len().checked_sub(1) else { return false };
        let mut matched = 0;
        for c in loosely(text, written) {
            while matched > 0 && self.chars[matched] != c {
                matched = self.fallback[matched - 1];
            }
            if self.chars[matched] == c {
                if matched == last {
                    return true;
                }
                matched += 1;
            }
        }
        false
    }
}

/// The characters of `text` as looking past marks reads them.
fn loosely(text: &str, written: Written) -> impl Iterator<Item = char> + '_ {
    let mut after_space = false;
    decoded(text, written)
        .filter(|c| !MARKS.contains(c))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(move |&c| {
            let first = c != ' ' || !after_space;
            after_space = c == ' ';
            first
        })
        .flat_map(char::to_lowercase)
}

/// The characters `text` stands for, written as `written` says.
fn decoded(text: &str, written: Written) -> impl Iterator<Item = char> + '_ {
    let mut chars = text.chars();
    std::iter::from_fn(move || {
        let c = chars.next()?;
        if written == Written::Plain || c != '\\' {
            return Some(c);
        }
        Some(match chars.next() {
            Some('n' | 'r' | 't') => ' ',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or(char::REPLACEMENT_CHARACTER)
            }
            // A quote, a backslash or a slash.
            Some(other) => other,
            None => c,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str, written: Written) -> String {
        loosely(text, written).collect()
    }

    #[test]
    fn marks_go_whitespace_is_one_space_and_case_is_lower() {
        assert_eq!(read("The **Parser**\n  is _fixed_", Written::Plain), "the parser is fixed");
        assert_eq!(read("a * b", Written::Plain), "a b", "a mark between spaces leaves one");
    }

    #[test]
    fn json_s_escapes_are_what_they_stand_for() {
        assert_eq!(read(r#"say \"hi\"\nthen\tstop é"#, Written::Json), "say \"hi\" then stop é");
        assert_eq!(read(r#"say \"hi\"\n"#, Written::Plain), "say \"hi\"n", "and as written, they're what's there");
    }

    #[test]
    fn a_partial_match_carries_on_from_where_it_can() {
        let loose = Loose::of("aab");

        assert!(loose.is_in("aaab", Written::Plain));
        assert!(!loose.is_in("abab", Written::Plain));
        assert!(Loose::of("abab").is_in("ababab", Written::Plain));
        assert!(!Loose::of("**").is_in("**", Written::Plain), "nothing's left of a needle of marks");
    }
}
