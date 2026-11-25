//! What each field that can be dictated into picks its hint words from: what's typed in it, what it's about, such as
//! its question, and the workspace and the chat it's in.

use hint_words::{Picked, PickedFrom};
use protocol::capability::asking::Question;
use protocol::model::{Proposed, PullRequest};

use super::{pick, picked_from};
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
    /// The words to favour, with what they were picked from.
    pub fn picked(&self) -> Picked {
        let from = self.picked_from();
        Picked { words: pick(&from, self.chat()), from }
    }

    fn picked_from(&self) -> PickedFrom {
        match self {
            Self::Composer { typed, attached, open_file, around } => PickedFrom {
                typed: (*typed).to_string(),
                attached: attached.to_vec(),
                open_file: open_file.map(str::to_string),
                ..picked_from::around(around)
            },
            Self::Answer { typed, question, around } => PickedFrom {
                typed: (*typed).to_string(),
                question: Some(picked_from::question(question)),
                ..picked_from::around(around)
            },
            Self::ProposalNote { typed, proposal, around } => PickedFrom {
                typed: typed.unwrap_or_default().to_string(),
                proposal: Some(picked_from::proposal(proposal)),
                ..picked_from::around(around)
            },
            Self::NewWorkspace { typed, attached, chosen_repo, repos } => PickedFrom {
                typed: (*typed).to_string(),
                attached: attached.to_vec(),
                repos: chosen_repo.iter().chain(repos).map(ToString::to_string).collect(),
                ..PickedFrom::default()
            },
            Self::DiffComment { typed, path, changed } => PickedFrom {
                typed: (*typed).to_string(),
                comment_on: Some((*path).to_string()),
                changed: picked_from::paths(changed),
                ..PickedFrom::default()
            },
        }
    }

    fn chat(&self) -> Option<&SessionLog> {
        match self {
            Self::Composer { around, .. } | Self::Answer { around, .. } | Self::ProposalNote { around, .. } => {
                around.chat
            }
            Self::NewWorkspace { .. } | Self::DiffComment { .. } => None,
        }
    }
}
