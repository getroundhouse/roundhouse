//! What a field's hint words were picked from.

use serde::{Deserialize, Serialize};

/// All that a field's hint words are picked from but the chat's messages, which the hub has: only which of them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PickedFrom {
    /// What was typed in the field already.
    pub typed: String,
    /// The names of the files attached to what's being written.
    pub attached: Vec<String>,
    /// The file the person had open, by its path.
    pub open_file: Option<String>,
    /// The question it answers.
    pub question: Option<Asked>,
    /// The proposal it's a note on.
    pub proposal: Option<Proposal>,
    /// The file it's a comment on, by its path.
    pub comment_on: Option<String>,
    /// The repositories a new workspace could be made in, by name, the one chosen first.
    pub repos: Vec<String>,
    pub workspace: Option<Workspace>,
    /// The workspace's changed files, by their paths.
    pub changed: Vec<String>,
    /// The chat's events the app had loaded, in order, as runs of their places in it.
    pub chat: Vec<Places>,
}

/// A question an agent asked, or one of a proposal's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub header: Option<String>,
    pub text: String,
    pub options: Vec<Choice>,
}

/// One of a question's options.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub title: String,
    pub questions: Vec<Asked>,
    /// Each part of it that's Markdown, in the order it's read.
    pub markdown: Vec<String>,
}

/// The workspace a field is in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    /// Its own branch, then the one checked out and each its worktree has had, as far as they're known.
    pub branches: Vec<String>,
    /// Its repository's name, if it's known.
    pub repo: Option<String>,
    /// The files its diff has comments on, by their paths, a path for each comment, the newest first.
    pub commented: Vec<String>,
    /// Its pull request's title, if it has one.
    pub pull_request: Option<String>,
}

/// A run of a chat's events, by the places of its first and its last, counting from 1.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Places {
    pub first: u64,
    pub last: u64,
}
