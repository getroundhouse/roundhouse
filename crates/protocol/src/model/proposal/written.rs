use serde::{Deserialize, Serialize};

/// What the agent wrote: a title and its context, then each question with its options. The structure is what lets a
/// client draw an outline, each option as a card, and a choice for each question, rather than finding them among a
/// document's headings. Its questions are named as [`Question`](crate::capability::asking::Question)'s are, and hold
/// what only a proposal's do: the details under the question, each option in full, and the agent's recommendation.
/// Nothing here is checked as it's read, and [`Proposed::check`] says whether one can be made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Proposed {
    /// What it's about, in a few words, as a tab's name.
    pub title: String,
    /// Markdown, before the first question: what's being decided, and what the person needs to know to decide it.
    #[serde(default)]
    pub context: String,
    /// In the order they're read and answered.
    pub questions: Vec<ProposalQuestion>,
}

impl Proposed {
    /// Each part of it that's Markdown, in the order it's read: the context, then each question's details, each
    /// option's body and why the agent recommends one.
    pub fn markdown(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.context.as_str()).chain(self.questions.iter().flat_map(ProposalQuestion::markdown))
    }
}

impl ProposalQuestion {
    /// Each part of it that's Markdown, in the order it's read.
    fn markdown(&self) -> impl Iterator<Item = &str> {
        let bodies = self.options.iter().map(|option| option.body.as_str());
        let why = self.recommended.iter().map(|recommended| recommended.why.as_str());
        std::iter::once(self.details.as_str()).chain(bodies).chain(why)
    }
}

/// One question the person answers, by choosing one of two to four options.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposalQuestion {
    /// A few words, which the outline and the answer bar name it by, as a question's header is.
    pub header: String,
    /// What's being decided, as a question.
    pub text: String,
    /// Markdown between the question and the options, such as a table that compares them.
    #[serde(default)]
    pub details: String,
    /// In the order they're read, which is the order they're named in, from A.
    pub options: Vec<ProposalOption>,
    /// The option the agent recommends, and why. `None` when it has no view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<Recommendation>,
}

/// One of a question's options, named as a [`QuestionOption`](crate::capability::asking::QuestionOption)'s is, with
/// the option in full besides.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposalOption {
    pub label: String,
    /// One line, under the label, which says what sets it apart.
    #[serde(default)]
    pub description: String,
    /// Markdown: the option in full, with what it costs.
    #[serde(default)]
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Recommendation {
    pub option: OptionPlace,
    /// Markdown.
    #[serde(default)]
    pub why: String,
}

/// An option, by its place among its question's options, from 0. The person and the agent name it by its letter,
/// which its question gives, as [`ProposalQuestion::letter`] says.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct OptionPlace(pub u8);

impl OptionPlace {
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// The letter the person and the agent name an option by, from A to Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionLetter(char);

impl OptionLetter {
    /// The letter `written` is, in upper or lower case, if it's one letter from A to Z.
    pub fn read(written: &str) -> Option<Self> {
        let mut chars = written.trim().chars();
        let (Some(only), None) = (chars.next(), chars.next()) else { return None };
        let upper = only.to_ascii_uppercase();
        upper.is_ascii_uppercase().then_some(Self(upper))
    }

    /// The letter at `place` in the alphabet, from A, if there is one.
    fn at(place: OptionPlace) -> Option<Self> {
        let letter = b'A'.checked_add(place.0).filter(u8::is_ascii_uppercase)?;
        Some(Self(char::from(letter)))
    }
}

impl std::fmt::Display for OptionLetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ProposalQuestion {
    /// The letter the option at `place` is named by, which is its place, from A, or `None` if there's no option there.
    pub fn letter(&self, place: OptionPlace) -> Option<OptionLetter> {
        self.options.get(place.index())?;
        OptionLetter::at(place)
    }

    /// The option `letter` names, if there's one.
    pub fn place_of(&self, letter: OptionLetter) -> Option<OptionPlace> {
        self.places().find(|place| self.letter(*place) == Some(letter))
    }

    /// The option at `place`, with its letter, if there's one.
    pub fn named(&self, place: OptionPlace) -> Option<(OptionLetter, &ProposalOption)> {
        Some((self.letter(place)?, self.options.get(place.index())?))
    }

    /// Each option's place, in the order they're read.
    pub fn places(&self) -> impl Iterator<Item = OptionPlace> + use<> {
        let count = u8::try_from(self.options.len()).unwrap_or(u8::MAX);
        (0..count).map(OptionPlace)
    }
}
