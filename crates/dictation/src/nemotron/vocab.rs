//! The model's vocabulary: the piece of text each of its tokens stands for, as `SentencePiece` makes them, where `▁`
//! starts a word.

use std::path::Path;

use crate::EngineError;

/// What starts a word in a piece.
const WORD_START: char = '\u{2581}';

pub(super) struct Vocabulary {
    /// Each token's piece, by its number.
    pieces: Vec<String>,
}

impl Vocabulary {
    /// The vocabulary in `path`, a JSON object from each token's number, as a string, to its piece.
    pub(super) fn read(path: &Path) -> Result<Self, EngineError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| EngineError(format!("couldn't read the vocabulary in {}: {e}", path.display())))?;
        Self::parse(&text)
    }

    pub(super) fn parse(text: &str) -> Result<Self, EngineError> {
        let bad = |why: &str| EngineError(format!("the vocabulary isn't as expected: {why}"));
        let by_number: std::collections::HashMap<String, String> =
            serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
        let numbered = || bad("its tokens aren't numbered from 0, one after another");
        let mut pieces = vec![None; by_number.len()];
        for (number, piece) in by_number {
            let number: usize = number.parse().map_err(|_| numbered())?;
            *pieces.get_mut(number).ok_or_else(numbered)? = Some(piece);
        }
        let pieces = pieces.into_iter().collect::<Option<Vec<_>>>().ok_or_else(numbered)?;
        Ok(Self { pieces })
    }

    /// The text of `token`, as it joins what came before it: `" word"` for a token that starts a word, and `"ing"`
    /// for one that goes on with it. Nothing for a token it doesn't have, or one that stands for no text.
    pub(super) fn text(&self, token: usize) -> &str {
        match self.pieces.get(token).map(String::as_str) {
            // Such as `<unk>`, for what the model couldn't make out.
            Some(piece) if piece.starts_with('<') && piece.ends_with('>') => "",
            Some(piece) => piece,
            None => "",
        }
    }

    /// The text of `tokens`, with each word start as a space.
    pub(super) fn join(&self, tokens: impl IntoIterator<Item = usize>) -> String {
        tokens.into_iter().map(|token| self.text(token)).collect::<String>().replace(WORD_START, " ")
    }

    /// `text`, a token's piece, with its word start as a space.
    pub(super) fn spaced(text: &str) -> String {
        text.replace(WORD_START, " ")
    }
}

#[cfg(test)]
mod tests {
    use super::Vocabulary;

    #[test]
    fn tokens_join_into_words_and_unknown_ones_say_nothing() {
        let vocabulary = Vocabulary::parse(r#"{"0": "<unk>", "1": "▁to", "2": "k", "3": "io", "4": "▁runs"}"#).unwrap();

        assert_eq!(vocabulary.join([1, 2, 3, 0, 4]), " tokio runs");
        assert_eq!(Vocabulary::spaced(vocabulary.text(4)), " runs");
        assert_eq!(vocabulary.text(9), "");
    }

    #[test]
    fn a_vocabulary_with_a_gap_in_its_numbers_is_refused() {
        assert!(Vocabulary::parse(r#"{"0": "a", "2": "b"}"#).is_err());
        assert!(Vocabulary::parse(r#"{"0": "a", "one": "b"}"#).is_err());
    }
}
