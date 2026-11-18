use protocol::capability::calls::{
    Call, CallOutput,
    call::{FilePatch, Patch, PatchChange, PatchHunk},
    output::FileOutcome,
};
use protocol::event::{FileChange, Hunk, HunkLine, LineMark};

use super::native::{Change, ChangeKind};

pub(super) fn patch(changes: Vec<Change>) -> (Call, CallOutput) {
    let mut files = Vec::new();
    let mut outcomes = Vec::new();
    for Change { path, diff, kind } in changes {
        let hunks = hunks(&diff);
        let (change, outcome, moved_to) = match kind {
            ChangeKind::Add => (PatchChange::Add { content: diff }, Some(FileChange::Created), None),
            ChangeKind::Delete => (PatchChange::Delete, None, None),
            ChangeKind::Update { move_path } => {
                let patch_hunks =
                    hunks.iter().map(|hunk| PatchHunk { after: None, lines: hunk.lines.clone() }).collect();
                (
                    PatchChange::Update { move_to: move_path.clone(), hunks: patch_hunks },
                    Some(FileChange::Changed { hunks }),
                    move_path,
                )
            }
            ChangeKind::Unknown => (PatchChange::Unknown, None, None),
        };
        files.push(FilePatch { path: path.clone(), change });
        outcomes.push(FileOutcome { path, change: outcome, moved_to, diagnostics: Vec::new() });
    }
    (Call::Patch(Patch { files }), CallOutput::Changed { files: outcomes })
}

/// Each file a diff in git's form changes, as Codex's turn's diff has them: a file it made as made, one it changed with
/// its hunks, one it moved with where to, and one it deleted with nothing said of its change, since nothing says that.
pub(in crate::codex) fn turn_changes(diff: &str) -> Vec<FileOutcome> {
    diff.split("\ndiff --git ").filter(|file| !file.trim().is_empty()).filter_map(file_outcome).collect()
}

fn file_outcome(file: &str) -> Option<FileOutcome> {
    let header = |prefix: &str| file.lines().find_map(|line| line.strip_prefix(prefix)).map(unquoted);
    let (from, to) =
        (header("--- a/").or_else(|| header("rename from ")), header("+++ b/").or_else(|| header("rename to ")));
    let path = from.clone().or_else(|| to.clone())?;
    let change = if file.lines().any(|line| line.starts_with("new file mode")) {
        Some(FileChange::Created)
    } else if file.lines().any(|line| line.starts_with("deleted file mode")) {
        None
    } else {
        Some(FileChange::Changed { hunks: hunks(file) })
    };
    let moved_to = to.filter(|to| from.as_ref().is_some_and(|from| from != to));
    Some(FileOutcome { path, change, moved_to, diagnostics: Vec::new() })
}

/// A path as git writes it, in quotes when it has a character git escapes, which leaves it as it's written within them.
fn unquoted(path: &str) -> String {
    path.trim_end_matches('\t').trim_matches('"').to_owned()
}

fn hunks(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ ") {
            let mut parts = header.split_whitespace();
            let number = |part: &str| part.get(1..)?.split(',').next()?.parse().ok();
            if let (Some(old), Some(new)) = (parts.next().and_then(number), parts.next().and_then(number)) {
                hunks.push(Hunk { old_start: old, new_start: new, lines: Vec::new() });
            }
        } else if let Some(hunk) = hunks.last_mut() {
            let mark = match line.as_bytes().first() {
                Some(b' ') => LineMark::Same,
                Some(b'+') => LineMark::Added,
                Some(b'-') => LineMark::Removed,
                _ => continue,
            };
            hunk.lines.push(HunkLine { mark, text: line[1..].into() });
        }
    }
    hunks
}
