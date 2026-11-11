//! What a workspace or a chat is called on screen, and where that came from.

use serde::{Deserialize, Deserializer, Serialize};

use super::one_line::one_line;

#[cfg(test)]
mod laws;

/// A title: one line of single spaces, never empty, with no control characters and none that hide or reorder text,
/// so a sidebar row shows it as it reads. Every way to make one tidies the text first, including reading one back,
/// so a title from before these rules still holds to them.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct Title(String);

impl Title {
    /// Room for a sentence, though a sidebar row shows far less. Past this it's more likely a paste by mistake.
    pub const MOST_TYPED_CHARS: usize = 120;

    /// A title someone typed. Line breaks become spaces, so a pasted one can't reach a sidebar row.
    pub fn typed(text: &str) -> Result<Self, TitleError> {
        let title = Self::tidied(text).ok_or(TitleError::Empty)?;
        if title.0.chars().count() > Self::MOST_TYPED_CHARS {
            return Err(TitleError::TooLong { most: Self::MOST_TYPED_CHARS });
        }
        Ok(title)
    }

    /// The first line of `text` with anything in it, cut at a word if it's long. A prompt titles its chat this way,
    /// and so does a model asked for a title, in case it rambles.
    pub fn from_first_line(text: &str) -> Option<Self> {
        const MOST: usize = 48;
        let line = text.split(breaks_line).find_map(Self::tidied)?;
        if line.0.chars().count() <= MOST {
            return Some(line);
        }
        let cut: String = line.0.chars().take(MOST).collect();
        let shortened = cut.rsplit_once(' ').map_or(cut.as_str(), |(start, _)| start);
        Some(Self(format!("{}…", shortened.trim_end())))
    }

    /// A title as it was stored or sent, tidied, or `None` if nothing is left. Its length isn't checked, so one
    /// from an older hub still reads.
    pub fn tidied(text: &str) -> Option<Self> {
        let words = one_line(text, usize::MAX);
        (!words.is_empty()).then_some(Self(words))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reads an optional title leniently: one with nothing left once tidied reads as no title, rather than failing the
    /// whole record.
    pub(crate) fn deserialize_optional<'de, D: Deserializer<'de>>(from: D) -> Result<Option<Self>, D::Error> {
        Ok(Option::<String>::deserialize(from)?.as_deref().and_then(Self::tidied))
    }
}

impl std::ops::Deref for Title {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a typed title was refused. The message is what the person sees.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleError {
    #[error("a title can't be empty")]
    Empty,
    #[error("a title can be at most {most} characters")]
    TooLong { most: usize },
}

/// Everything Unicode counts as a line break, not only `\n`, so a lone `\r` or a line separator can't hide a second
/// line in a title.
fn breaks_line(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Where a workspace's or a chat's title came from, so one typed by hand is replaced only when someone asks for a
/// new one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    /// The first line of the first prompt, until a generated title replaces it. Also what a record with no
    /// title says.
    #[default]
    FirstLine,
    Generated,
    /// Nothing generated replaces it, unless someone asks for a new title.
    ByHand,
    /// Treated like [`TitleSource::ByHand`], so a title nobody can account for is left alone.
    #[serde(other)]
    Unknown,
}
