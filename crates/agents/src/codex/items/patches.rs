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
