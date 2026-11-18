//! What the model is told of the language a recording is heard in: the prompt its encoders are given, and the alphabet
//! what's heard is written in. The prompt only leans the model toward the language. On a short phrase it can't make out, it can still choose
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

/// What the model is told of a recording's language: its prompt's number, as the model's `metadata.json` numbers them,
/// and the alphabet it's written in, unless the model is left to tell.
pub(super) struct Prompt {
    pub(super) number: i32,
    alphabet: Option<Alphabet>,
}

impl Prompt {
    pub(super) fn of(language: crate::Language) -> Self {
        use crate::Language as L;
        let (number, alphabet) = match language {
            L::Auto => return Self { number: 127, alphabet: None },
            L::Bulgarian => (30, Alphabet::Cyrillic),
            L::Croatian => (29, Alphabet::Latin),
            L::Czech => (22, Alphabet::Latin),
            L::Danish => (25, Alphabet::Latin),
            L::Dutch => (16, Alphabet::Latin),
            L::English => (0, Alphabet::Latin),
            L::Estonian => (60, Alphabet::Latin),
            L::Finnish => (26, Alphabet::Latin),
            L::French => (8, Alphabet::Latin),
            L::German => (9, Alphabet::Latin),
            L::Greek => (21, Alphabet::Greek),
            L::Hungarian => (23, Alphabet::Latin),
            L::Italian => (15, Alphabet::Latin),
            L::Latvian => (61, Alphabet::Latin),
            L::Lithuanian => (31, Alphabet::Latin),
            L::Maltese => (102, Alphabet::Latin),
            L::Polish => (17, Alphabet::Latin),
            L::Portuguese => (13, Alphabet::Latin),
            L::Romanian => (20, Alphabet::Latin),
            L::Russian => (11, Alphabet::Cyrillic),
            L::Slovak => (28, Alphabet::Latin),
            L::Slovenian => (62, Alphabet::Latin),
            L::Spanish => (3, Alphabet::Latin),
            L::Swedish => (24, Alphabet::Latin),
            L::Ukrainian => (19, Alphabet::Cyrillic),
        };
        Self { number, alphabet: Some(alphabet) }
    }
}

/// The tokens a recording isn't heard as, for each alphabet, worked out once from the vocabulary.
pub(super) struct LeftOut {
    latin: OtherAlphabets,
    cyrillic: OtherAlphabets,
    greek: OtherAlphabets,
}

impl LeftOut {
    pub(super) fn new(vocabulary: &Vocabulary) -> Self {
        let other_than = |alphabet| OtherAlphabets::new(vocabulary, alphabet);
        Self {
            latin: other_than(Alphabet::Latin),
            cyrillic: other_than(Alphabet::Cyrillic),
            greek: other_than(Alphabet::Greek),
        }
    }

    /// Those a recording `prompt` is given isn't heard as: none, when the model is left to tell its language.
    pub(super) fn of(&self, prompt: &Prompt) -> &OtherAlphabets {
        match prompt.alphabet {
            None => &LEAVES_OUT_NONE,
            Some(Alphabet::Latin) => &self.latin,
            Some(Alphabet::Cyrillic) => &self.cyrillic,
            Some(Alphabet::Greek) => &self.greek,
        }
    }
}

/// What a recording whose language the model is left to tell leaves out, which lasts as long as what it's of does.
static LEAVES_OUT_NONE: OtherAlphabets = OtherAlphabets::NONE;

/// The tokens whose pieces have a letter outside an alphabet. Pieces with no letters, such as punctuation, digits and
/// `<unk>`, are in every alphabet.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct OtherAlphabets(Vec<usize>);

impl OtherAlphabets {
    /// None.
    pub(super) const NONE: Self = Self(Vec::new());

    fn new(vocabulary: &Vocabulary, alphabet: Alphabet) -> Self {
        let in_another = |text: &str| {
            text.chars().filter(|c| c.is_alphabetic()).any(|letter| Alphabet::of(letter) != Some(alphabet))
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
    use std::collections::HashSet;

    use super::{Alphabet, LeftOut, OtherAlphabets, Prompt};
    use crate::Language;
    use crate::nemotron::vocab::Vocabulary;

    fn vocabulary(pieces: &[&str]) -> Vocabulary {
        let json = pieces.iter().enumerate().map(|(id, piece)| format!("\"{id}\": \"{piece}\"")).collect::<Vec<_>>();
        Vocabulary::parse(&format!("{{{}}}", json.join(", "))).unwrap()
    }

    #[test]
    fn each_alphabet_leaves_out_the_pieces_with_another_alphabets_letters_and_keeps_those_with_none() {
        let pieces = ["<unk>", "▁the", "▁café", "ł", "ß", "▁Та", "й", "▁το", "ά", "▁", ".", "1", "€", "▁tak▁так"];
        let vocabulary = vocabulary(&pieces);

        assert_eq!(OtherAlphabets::new(&vocabulary, Alphabet::Latin), OtherAlphabets(vec![5, 6, 7, 8, 13]));
        assert_eq!(OtherAlphabets::new(&vocabulary, Alphabet::Cyrillic), OtherAlphabets(vec![1, 2, 3, 4, 7, 8, 13]));
        assert_eq!(OtherAlphabets::new(&vocabulary, Alphabet::Greek), OtherAlphabets(vec![1, 2, 3, 4, 5, 6, 13]));
    }

    #[test]
    fn a_language_leaves_out_the_other_alphabets_and_auto_leaves_out_none() {
        let left_out = LeftOut::new(&vocabulary(&["▁the", "▁Та", "▁το"]));
        let of = |language| left_out.of(&Prompt::of(language));

        assert_eq!(of(Language::English), &OtherAlphabets(vec![1, 2]));
        assert_eq!(of(Language::Ukrainian), &OtherAlphabets(vec![0, 2]));
        assert_eq!(of(Language::Greek), &OtherAlphabets(vec![0, 1]));
        assert_eq!(of(Language::Auto), &OtherAlphabets::NONE);
    }

    #[test]
    fn each_language_has_a_prompt_of_its_own() {
        let numbers: HashSet<i32> = Language::ALL.into_iter().map(|language| Prompt::of(language).number).collect();
        assert_eq!(numbers.len(), Language::ALL.len());
        assert_eq!(Prompt::of(Language::English).number, 0);
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
