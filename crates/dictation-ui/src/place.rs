//! Which of a host's fields is dictated to.

/// Which field dictation is attached to, which says how its line is drawn, and which the log says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Composer,
    NewWorkspace,
    DiffComment,
    /// What's typed in answer to an agent's question.
    Answer,
    ProposalNote,
}

impl Place {
    /// The corner of what the fine line runs along.
    pub(crate) fn corner(self) -> look::Corner {
        match self {
            Place::Composer | Place::NewWorkspace | Place::ProposalNote => look::Corner::Card,
            Place::DiffComment | Place::Answer => look::Corner::Control,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Place::Composer => "composer",
            Place::NewWorkspace => "new_workspace",
            Place::DiffComment => "diff_comment",
            Place::Answer => "answer",
            Place::ProposalNote => "proposal_note",
        }
    }
}
