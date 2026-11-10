//! A worktree's files, as a client browses them: what's in it or in one of its folders, and what one file holds.

use serde::{Deserialize, Serialize};

/// Files in a worktree, tracked, new or ignored, by their paths from the worktree, and the folders whose files are only
/// listed once someone asks, since one such as `target` or `node_modules` can hold hundreds of thousands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct WorktreeFiles {
    /// In order.
    pub paths: Vec<String>,
    /// Folders whose files aren't in `paths`, by their paths from the worktree, in order. Reading one lists what's
    /// directly in it.
    #[serde(default)]
    pub folders: Vec<String>,
    /// True when some were left out to keep the reply bounded.
    pub truncated: bool,
}

/// What a file in a worktree holds, as a client shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileContents {
    /// Its text. Bytes that aren't UTF-8 are replaced, so it can still be shown.
    Text { text: String },
    /// It's something other than text, by a NUL byte near its start, as git decides.
    Binary,
    /// It's bigger than a client is sent.
    TooBig { bytes: u64 },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::FileContents;

    /// A newer hub can say something this build doesn't know, which has to read as something it can show.
    #[test]
    fn contents_of_a_kind_this_build_doesn_t_know_read_as_unknown() {
        let text: FileContents = serde_json::from_str(r#"{"kind":"text","text":"one"}"#).unwrap();
        let newer: FileContents = serde_json::from_str(r#"{"kind":"image","media_type":"png"}"#).unwrap();

        assert_eq!(text, FileContents::Text { text: "one".to_string() });
        assert_eq!(newer, FileContents::Unknown);
    }
}
