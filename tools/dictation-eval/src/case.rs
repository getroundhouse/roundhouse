//! A case: what's around a field someone dictates into, what they then say, and the terms in it they'd want spelled
//! right. It holds what the person could see, even what no field hints yet, such as the lines a comment is on, so
//! a change that starts hinting it shows in the scores.

use protocol::capability::calls::Call;
use serde::Deserialize;

/// The kind of field a case dictates into, as `client_core::hints::Field` has them.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Composer,
    Answer,
    ProposalNote,
    NewWorkspace,
    DiffComment,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub field: FieldKind,
    /// What the person says into the field, as it should be written, with each term as its `written` spelling.
    pub said: String,
    /// What's typed in the field already when they start.
    #[serde(default)]
    pub typed: String,
    /// The terms in what's said that they'd want spelled right, which plain speech has none of.
    #[serde(default)]
    pub terms: Vec<Term>,
    /// The names of the files attached to what's being written.
    #[serde(default)]
    pub attached: Vec<String>,
    /// The file the person has open, if one is.
    #[serde(default)]
    pub open_file: Option<String>,
    #[serde(default)]
    pub workspace: Option<WorkspaceCase>,
    /// The chat the field is in, oldest first.
    #[serde(default)]
    pub chat: Vec<ChatEntry>,
    #[serde(default)]
    pub question: Option<QuestionCase>,
    #[serde(default)]
    pub proposal: Option<ProposalCase>,
    #[serde(default)]
    pub comment: Option<CommentCase>,
    #[serde(default)]
    pub repos: Option<ReposCase>,
}

/// A term the person says, which counts as spelled right written as `written` or as any of `also`, such as how
/// `dwell_ms` is said aloud, "dwell ms".
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub written: String,
    #[serde(default)]
    pub also: Vec<String>,
}

impl Term {
    pub fn spellings(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.written.as_str()).chain(self.also.iter().map(String::as_str))
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCase {
    pub name: String,
    pub branch: String,
    pub repo: String,
    /// Its pull request's title, if it has one.
    #[serde(default)]
    pub pull_request: Option<String>,
    /// The paths of its changed files.
    #[serde(default)]
    pub changed: Vec<String>,
}

/// One entry in a chat: what the person wrote, what the agent wrote, or a tool call the agent made, as `protocol`
/// writes one, such as `call = { kind = "read", path = "src/relay.rs" }`. A call's kind is checked, and its keys
/// aren't: `protocol` drops one it doesn't know, so a misspelt one is lost without a word.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatEntry {
    Person(String),
    Agent(String),
    Call(Call),
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct QuestionCase {
    #[serde(default)]
    pub header: Option<String>,
    pub text: String,
    pub options: Vec<QuestionOptionCase>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct QuestionOptionCase {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProposalCase {
    pub title: String,
    /// The Markdown above its questions.
    #[serde(default)]
    pub context: String,
    pub questions: Vec<ProposalQuestionCase>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProposalQuestionCase {
    pub header: String,
    pub text: String,
    #[serde(default)]
    pub details: String,
    pub options: Vec<ProposalOptionCase>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProposalOptionCase {
    pub label: String,
    pub description: String,
    #[serde(default)]
    pub body: String,
}

/// The comment being written on a line of the diff.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct CommentCase {
    /// The path of the file it's on.
    pub path: String,
    /// The lines it's on, and a few either side.
    #[serde(default)]
    pub lines: Vec<String>,
    /// The file's other added lines.
    #[serde(default)]
    pub added: Vec<String>,
}

/// The repositories a new workspace can be made in.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ReposCase {
    #[serde(default)]
    pub chosen: Option<String>,
    pub offered: Vec<String>,
}

/// Why a case can't be scored.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    #[error("a {0:?} case needs a [{1}] table")]
    Needs(FieldKind, &'static str),
    #[error("its chat's entry {0}, from 1, is a call of a kind protocol doesn't know")]
    UnknownCall(usize),
    #[error("{0:?} is a term, but isn't in what's said")]
    NotSaid(String),
}

impl Case {
    /// Checks what's in the case itself: its chat's calls are kinds `protocol` knows, so a misspelt kind isn't read as
    /// a call that says nothing, and each term is in what's said. Whether it has what its field needs is checked as
    /// the field is made.
    pub fn check(&self) -> Result<(), Invalid> {
        if let Some(at) = self.chat.iter().position(|entry| matches!(entry, ChatEntry::Call(Call::Unknown))) {
            return Err(Invalid::UnknownCall(at + 1));
        }
        match self.terms.iter().find(|term| !self.said.contains(&term.written)) {
            Some(term) => Err(Invalid::NotSaid(term.written.clone())),
            None => Ok(()),
        }
    }
}
