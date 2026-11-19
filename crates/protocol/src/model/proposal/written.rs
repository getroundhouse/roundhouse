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

/// An option, by its place among its question's options, from 0, which the person reads as a letter, from A.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct OptionPlace(pub u8);

impl OptionPlace {
    /// The letter the person and the agent name it by.
    pub fn letter(self) -> char {
        char::from(b'A'.saturating_add(self.0))
    }

    /// The place `letter` names, in upper or lower case, if it names one.
    pub fn of_letter(letter: &str) -> Option<Self> {
        let mut chars = letter.trim().chars();
        let (Some(only), None) = (chars.next(), chars.next()) else { return None };
        let upper = u8::try_from(only.to_ascii_uppercase()).ok()?;
        upper.is_ascii_uppercase().then(|| OptionPlace(upper - b'A'))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}
