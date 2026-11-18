//! The language a recording is heard in: the prompt its encoders are given, and the alphabet what's heard is written
//! in. The prompt only leans the model toward the language. On a short phrase it can't make out, it can still choose
//! pieces in another language's alphabet, NVIDIA's own decoding included, so the decoder leaves those pieces out.

use super::vocab::Vocabulary;

/// An alphabet the model's pieces are written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Alphabet {
    Latin,
    Cyrillic,
    Greek,
}

impl Alphabet {
    /// The alphabet `letter` is in, or `None` for one in none of the model's, by the Unicode blocks each is in.
    fn of(letter: char) -> Option<Self> {
        match u32::from(letter) {
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Some(Self::Greek),
            0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Some(Self::Cyrillic),
            0x0041..=0x005A | 0x0061..=0x007A | 0x00AA | 0x00BA | 0x00C0..=0x024F | 0x1E00..=0x1EFF => {
                Some(Self::Latin)
            }
            _ => None,
        }
    }
}

/// A language the model hears: its prompt's number, as the model's `metadata.json` numbers them, and its alphabet.
pub(super) struct Language {
    pub(super) prompt: i32,
    alphabet: Alphabet,
}

/// English, the first of the model's prompts.
pub(super) const ENGLISH: Language = Language { prompt: 0, alphabet: Alphabet::Latin };

/// The tokens whose pieces have a letter outside a language's alphabet. Pieces with no letters, such as punctuation,
/// digits and `<unk>`, are in every language.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct OtherAlphabets(Vec<usize>);

impl OtherAlphabets {
    /// None, for tests of what doesn't depend on the alphabet.
    #[cfg(test)]
    pub(super) const NONE: Self = Self(Vec::new());

    pub(super) fn new(vocabulary: &Vocabulary, language: &Language) -> Self {
        let in_another = |text: &str| {
            text.chars().filter(|c| c.is_alphabetic()).any(|letter| Alphabet::of(letter) != Some(language.alphabet))
        };
        Self((0..vocabulary.pieces().len()).filter(|&token| in_another(vocabulary.text(token))).collect())
    }

    /// Takes every chance away from these tokens in `row`, one step's log-probabilities for each token. The rest keep
    /// theirs as they were, without being normalised again, so a step the model heard as another alphabet's piece
    /// still scores as poorly as the model thought it fit.
    pub(super) fn leave_out(&self, row: &mut [f32]) {
        for &token in &self.0 {
            if let Some(chance) = row.get_mut(token) {
                *chance = f32::NEG_INFINITY;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Alphabet, ENGLISH, OtherAlphabets};
    use crate::nemotron::vocab::Vocabulary;

    #[test]
    fn english_leaves_out_the_pieces_with_cyrillic_or_greek_letters_and_keeps_those_with_none() {
        let pieces = ["<unk>", "▁the", "▁café", "ł", "ß", "▁Та", "й", "▁το", "ά", "▁", ".", "1", "€", "▁tak▁так"];
        let json = pieces.iter().enumerate().map(|(id, piece)| format!("\"{id}\": \"{piece}\"")).collect::<Vec<_>>();
        let vocabulary = Vocabulary::parse(&format!("{{{}}}", json.join(", "))).unwrap();

        assert_eq!(OtherAlphabets::new(&vocabulary, &ENGLISH), OtherAlphabets(vec![5, 6, 7, 8, 13]));
    }

    #[test]
    fn a_letter_in_none_of_the_models_alphabets_is_in_none() {
        assert_eq!(Alphabet::of('ж'), Some(Alphabet::Cyrillic));
        assert_eq!(Alphabet::of('ἀ'), Some(Alphabet::Greek));
        assert_eq!(Alphabet::of('ő'), Some(Alphabet::Latin));
        assert_eq!(Alphabet::of('ש'), None);
    }

    #[test]
    fn a_left_out_token_has_no_chance_and_the_others_keep_theirs() {
        let left_out = OtherAlphabets(vec![1, 9]);
        let mut row = [-1.0, -2.0, -3.0];

        left_out.leave_out(&mut row);

        assert_eq!(row, [-1.0, f32::NEG_INFINITY, -3.0]);
    }
}
