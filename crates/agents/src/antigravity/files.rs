//! Where `agy` keeps a conversation on its machine, under the home's `.gemini/antigravity-cli/`, as
//! `docs/reference/antigravity-cli.md` says. The adapter reads it to tell a conversation `agy` no longer has from one it can't
//! read, to notice a between-turn reactive wakeup `agy` 1.5.9 hasn't streamed over ACP, and to remove a title call's
//! files, which nobody resumes.

use std::path::{Path, PathBuf};

use protocol::model::AgentSessionId;

use crate::AgentOptions;

pub(super) struct ConversationFiles {
    db: PathBuf,
    annotations: PathBuf,
    brain: PathBuf,
}

/// The home directory the launched `agy` process uses: an explicit `HOME`/`USERPROFILE` override in `env`, or the
/// worker's configured home.
pub(super) fn effective_home<'a>(env: &'a [(String, String)], options: &'a AgentOptions) -> Option<&'a Path> {
    env.iter()
        .rev()
        .find(|(key, _)| key == protocol::paths::HOME_VAR)
        .map(|(_, value)| Path::new(value.as_str()))
        .or(options.binary.home.as_deref())
}

impl ConversationFiles {
    /// `conversation`'s under `home`, or `None` for an ID that could name a path outside them, since it comes from the
    /// hub's record of what `agy` said.
    pub(super) fn of(home: &Path, conversation: &AgentSessionId) -> Option<Self> {
        let id = conversation.as_str();
        if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
            return None;
        }
        let root = home.join(".gemini").join("antigravity-cli");
        Some(Self {
            db: root.join("conversations").join(format!("{id}.db")),
            annotations: root.join("annotations").join(format!("{id}.pbtxt")),
            brain: root.join("brain").join(id),
        })
    }

    /// The JSONL trajectory log `agy` appends every user, model and `<SYSTEM_MESSAGE>` wakeup step to.
    pub(super) fn transcript(&self) -> PathBuf {
        self.brain.join(".system_generated").join("logs").join("transcript.jsonl")
    }

    /// Whether the conversation's own file is gone. One that can't be looked for counts as there, so nothing is taken
    /// as gone that might not be.
    pub(super) async fn gone(&self) -> bool {
        tokio::fs::try_exists(&self.db).await.is_ok_and(|exists| !exists)
    }

    /// Removes them, each as far as it's there.
    pub(super) async fn remove(&self) {
        let _ = tokio::fs::remove_file(&self.db).await;
        let _ = tokio::fs::remove_file(&self.annotations).await;
        let _ = tokio::fs::remove_dir_all(&self.brain).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_conversations_files_are_removed_and_an_id_that_leaves_them_names_none() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".gemini").join("antigravity-cli");
        tokio::fs::create_dir_all(root.join("conversations")).await.unwrap();
        tokio::fs::create_dir_all(root.join("annotations")).await.unwrap();
        tokio::fs::create_dir_all(root.join("brain").join("s1")).await.unwrap();
        tokio::fs::write(root.join("conversations").join("s1.db"), b"db").await.unwrap();
        tokio::fs::write(root.join("conversations").join("keep.db"), b"keep").await.unwrap();
        tokio::fs::write(root.join("annotations").join("s1.pbtxt"), b"ann").await.unwrap();
        tokio::fs::write(root.join("brain").join("s1").join("note.md"), b"n").await.unwrap();
        let files = ConversationFiles::of(dir.path(), &AgentSessionId::from("s1")).unwrap();
        assert!(!files.gone().await);

        files.remove().await;

        assert!(ConversationFiles::of(dir.path(), &AgentSessionId::from("../conversations/keep")).is_none());
        assert!(ConversationFiles::of(dir.path(), &AgentSessionId::from("")).is_none());
        assert!(files.gone().await);
        assert!(!root.join("annotations").join("s1.pbtxt").exists());
        assert!(!root.join("brain").join("s1").exists());
        assert!(root.join("conversations").join("keep.db").exists());
    }
}
