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
    /// The letter the agent gave it, such as to keep the one it had in the revision before when an option before it
    /// was dropped. `None` takes the first letter no other option of its question has, as
    /// [`ProposalQuestion::letter`] says. A worker from before options had letters reads each option by its place
    /// instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter: Option<OptionLetter>,
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct OptionLetter(char);

impl TryFrom<String> for OptionLetter {
    type Error = String;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        Self::read(&written).ok_or_else(|| format!("{written:?} isn't a letter from A to Z"))
    }
}

impl From<OptionLetter> for String {
    fn from(letter: OptionLetter) -> Self {
        letter.to_string()
    }
}

impl OptionLetter {
    /// The letter `written` is, in upper or lower case, if it's one letter from A to Z.
    pub fn read(written: &str) -> Option<Self> {
        let mut chars = written.trim().chars();
        let (Some(only), None) = (chars.next(), chars.next()) else { return None };
        let upper = only.to_ascii_uppercase();
        upper.is_ascii_uppercase().then_some(Self(upper))
    }

    /// Every letter, from A to Z.
    fn all() -> impl Iterator<Item = Self> {
        ('A'..='Z').map(Self)
    }
}

impl std::fmt::Display for OptionLetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ProposalQuestion {
    /// The letter the option at `place` is named by, or `None` if there's no option there: the one the agent gave it,
    /// or else the first that no option of the question was given, and no option before it takes. So options the agent
    /// gave none are named A, B, C in order, as they always were.
    pub fn letter(&self, place: OptionPlace) -> Option<OptionLetter> {
        self.letters().nth(place.index())
    }

    /// Each option's letter, in the order they're read.
    fn letters(&self) -> impl Iterator<Item = OptionLetter> + '_ {
        let given: Vec<OptionLetter> = self.options.iter().filter_map(|option| option.letter).collect();
        let mut free = OptionLetter::all().filter(move |letter| !given.contains(letter));
        // Past Z, which no question gets near, an option takes Z again, which the check refuses.
        self.options.iter().map(move |option| option.letter.or_else(|| free.next()).unwrap_or(OptionLetter('Z')))
    }

    /// A letter two of its options are named by, which can't tell them apart.
    pub(super) fn letter_twice(&self) -> Option<OptionLetter> {
        let letters: Vec<OptionLetter> = self.letters().collect();
        letters.iter().enumerate().find(|(at, letter)| letters[..*at].contains(letter)).map(|(_, letter)| *letter)
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
