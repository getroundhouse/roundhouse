//! One case's score: which of its terms the field hints, as the engine takes the hint words, and how many of those
//! words favour none of its terms.

use std::collections::BTreeSet;

use dictation::{Hint, Hints};
use serde::{Deserialize, Serialize};

use crate::built::Built;
use crate::case::{Case, Invalid, Term};

/// A case's score. Every hint gets the same boost, so whether a term is hinted is what counts, and where it is among
/// the hints matters only to whether it survives a limit, so the score leaves that out.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Score {
    /// The terms the field hints, by how each is written.
    pub hinted: BTreeSet<String>,
    /// The terms it doesn't.
    pub missed: BTreeSet<String>,
    /// How many hint words the engine is given.
    pub hints: usize,
    /// How many of them favour none of the terms.
    pub unused: usize,
}

impl Case {
    pub fn score(&self) -> Result<Score, Invalid> {
        self.check()?;
        let built = Built::new(self);
        let words = built.field()?.hint_words();
        let hints = Hints::new(words.into_iter().map(|word| Hint { text: word.text, spelled: word.spelled }));
        Ok(Score::of(&self.terms, &hints))
    }
}

impl Score {
    /// Which of `terms` some of `hints` favours, by any of its spellings, and how many hints favour none of them.
    pub(crate) fn of(terms: &[Term], hints: &Hints) -> Self {
        let mut score = Self { hints: hints.words().len(), ..Self::default() };
        let mut used = BTreeSet::new();
        for term in terms {
            let favouring: BTreeSet<usize> =
                term.spellings().filter_map(|spelling| hints.favouring(spelling)).collect();
            let into = if favouring.is_empty() { &mut score.missed } else { &mut score.hinted };
            into.insert(term.written.clone());
            used.extend(favouring);
        }
        score.unused = score.hints - used.len();
        score
    }
}
