//! Which words each field that can be dictated into hints, and in what order: what's typed in it, then what it's
//! about, such as its question, then the workspace and the chat it's in.

use protocol::capability::asking::Question;
use protocol::model::{Proposed, PullRequest};

use super::HintCollector;
use crate::records::InWorkspace;
use crate::session_log::SessionLog;

/// A field that can be dictated into, in any client, and what around it its hint words come from.
pub enum Field<'a> {
    /// The workspace's composer, with the names of the files attached to the prompt, and the file the person has
    /// open, if one is.
    Composer { typed: &'a str, attached: &'a [String], open_file: Option<&'a str>, around: Around<'a> },
    /// A question's typed answer.
    Answer { typed: &'a str, question: &'a Question, around: Around<'a> },
    /// The note sent with a proposal's answer, with what's typed in it, if it has one yet.
    ProposalNote { typed: Option<&'a str>, proposal: &'a Proposed, around: Around<'a> },
    /// The prompt a new workspace starts with, with the repository chosen for it, then every one offered.
    NewWorkspace { typed: &'a str, attached: &'a [String], chosen_repo: Option<&'a str>, repos: Vec<&'a str> },
    /// A comment on a line of the diff, with its file's path, then every changed file's.
    DiffComment { typed: &'a str, path: &'a str, changed: Vec<&'a str> },
}

/// A word or phrase a field favours, most immediate first, as `dictation`'s `Hint` takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintWord {
    pub text: String,
    /// Whether it's written as it's hinted where it's hyphenated and its words are heard spaced or joined, as a
    /// package's name is. The workspace's own names, its branches', repositories' and files', aren't, since a person
    /// says their words in prose too, such as "the stale reports" on a branch `stale-reports`.
    pub spelled: bool,
}

/// The workspace and the chat a field is in, as far as they're known.
#[derive(Default)]
pub struct Around<'a> {
    pub workspace: Option<Workspace<'a>>,
    pub chat: Option<&'a SessionLog>,
}

/// What a workspace hints: its record, its pull request, then its changed files.
pub struct Workspace<'a> {
    pub shown: &'a InWorkspace,
    pub pull_request: Option<&'a PullRequest>,
    pub changed: Vec<&'a str>,
}

impl Field<'_> {
    /// The words to favour, most immediate first.
    pub fn hint_words(&self) -> Vec<HintWord> {
        let mut collector = HintCollector::new();
        match self {
            Self::Composer { typed, attached, open_file, around } => {
                collector.add_prose(typed);
                collector.add_paths(attached.iter().map(String::as_str));
                collector.add_paths(*open_file);
                collector.add_around(around);
            }
            Self::Answer { typed, question, around } => {
                collector.add_prose(typed);
                collector.add_question(question);
                collector.add_around(around);
            }
            Self::ProposalNote { typed, proposal, around } => {
                if let Some(typed) = typed {
                    collector.add_prose(typed);
                }
                collector.add_proposal(proposal);
                collector.add_around(around);
            }
            Self::NewWorkspace { typed, attached, chosen_repo, repos } => {
                collector.add_prose(typed);
                collector.add_paths(attached.iter().map(String::as_str));
                for name in chosen_repo.iter().chain(repos) {
                    collector.add_own_name(name);
                }
            }
            Self::DiffComment { typed, path, changed } => {
                collector.add_prose(typed);
                collector.add_path(path);
                collector.add_paths(changed.iter().copied());
            }
        }
        collector.finish()
    }
}

impl HintCollector {
    fn add_paths<'a>(&mut self, paths: impl IntoIterator<Item = &'a str>) {
        for path in paths {
            self.add_path(path);
        }
    }

    /// The workspace's words, then the chat's.
    fn add_around(&mut self, around: &Around<'_>) {
        if let Some(workspace) = &around.workspace {
            self.add_workspace(workspace.shown);
            if let Some(pull_request) = workspace.pull_request {
                self.add_pull_request(pull_request);
            }
            self.add_paths(workspace.changed.iter().copied());
        }
        if let Some(chat) = around.chat {
            self.add_session_log(chat);
        }
    }
}
