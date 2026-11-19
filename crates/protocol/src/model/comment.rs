use super::{CommentId, Rev, WorkspaceId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Old,
    New,
    #[serde(other)]
    Unknown,
}

/// A review comment on a changed line, for the agent. It stays until it's deleted.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DiffComment {
    pub id: CommentId,
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    pub text: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

impl DiffComment {
    /// The order a hub lists comments in, and a client shows a workspace's: oldest first, by when each was made, then
    /// by ID.
    pub fn oldest_first(&self, other: &DiffComment) -> std::cmp::Ordering {
        (self.created_at_ms, &self.id).cmp(&(other.created_at_ms, &other.id))
    }

    /// The comment as an agent reads it, on a line of its own: where it was left, and what it says. It says when it's
    /// on a removed line, since that line is no longer in the file.
    pub fn for_agent(&self) -> String {
        let side = match self.side {
            DiffSide::Old => " (a removed line)",
            DiffSide::New | DiffSide::Unknown => "",
        };
        format!("`{}:{}`{side}: {}", self.path, self.line, self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffComment, DiffSide};

    fn comment(path: &str, side: DiffSide, line: u32, text: &str) -> DiffComment {
        DiffComment {
            id: "c".into(),
            workspace_id: "w".into(),
            path: path.into(),
            side,
            line,
            text: text.into(),
            created_at_ms: 0,
            rev: super::Rev::default(),
        }
    }

    #[test]
    fn an_agent_reads_where_a_comment_is_and_whether_its_line_was_removed() {
        let added = comment("src/a.rs", DiffSide::New, 12, "Rename this");
        let removed = comment("src/b.rs", DiffSide::Old, 3, "Why remove this?");

        assert_eq!(added.for_agent(), "`src/a.rs:12`: Rename this");
        assert_eq!(removed.for_agent(), "`src/b.rs:3` (a removed line): Why remove this?");
    }
}
