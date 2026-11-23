//! Which of the changed files have comments, marked on their rows, so a file with comments can be found from the list
//! and opened to read them, edit them or add them to the chat.

use std::collections::{HashMap, HashSet};

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};
use look::{IconSizing as _, LineText};
use protocol::model::DiffComment;

use crate::review::ChangedRow;

/// How many comments each changed file has, by its path. A comment on a file the branch no longer changes has no row,
/// so it isn't counted.
pub(super) fn commented(files: &[ChangedRow], comments: &[DiffComment]) -> HashMap<String, usize> {
    let changed: HashSet<&str> = files.iter().map(|file| file.path.as_ref()).collect();
    let mut counts = HashMap::new();
    for comment in comments.iter().filter(|comment| changed.contains(comment.path.as_str())) {
        *counts.entry(comment.path.clone()).or_insert(0) += 1;
    }
    counts
}

/// How many comments the changed files in the folder at `folder` have, at any depth, for its row while it's closed.
pub(super) fn in_folder(commented: &HashMap<String, usize>, folder: &str) -> usize {
    let inside = format!("{folder}/");
    commented.iter().filter(|(path, _)| path.starts_with(&inside)).map(|(_, count)| count).sum()
}

/// A comment's icon and how many there are, in the muted colour the row's other icons are in, on the line counts'
/// baseline. Nothing when there are none.
pub(super) fn mark(path: &str, count: usize, line_text: &LineText, cx: &App) -> Option<AnyElement> {
    let muted = look::colors(cx).text_muted;
    (count > 0).then(|| {
        h_flex()
            .id(ElementId::Name(format!("comments-{path}").into()))
            .flex_none()
            .gap(look::Space::XSmall)
            .text_color(muted)
            .child(Icon::new(Lucide::MessageSquare).sized(look::IconSize::Small).text_color(muted))
            .child(line_text.code.set(div()).child(count.to_string()))
            .test_support()
            .into_any_element()
    })
}

#[cfg(test)]
mod tests {
    use super::{ChangedRow, commented, in_folder};
    use protocol::model::{DiffComment, DiffSide, Rev};

    fn comment(path: &str) -> DiffComment {
        DiffComment {
            id: path.into(),
            workspace_id: "w".into(),
            path: path.into(),
            side: DiffSide::New,
            line: 1,
            text: "Why?".into(),
            created_at_ms: 0,
            rev: Rev::default(),
        }
    }

    #[test]
    fn a_closed_folder_counts_the_comments_on_the_changed_files_under_it() {
        let row = |path: &'static str| ChangedRow {
            path: path.into(),
            status: crate::patch::FileStatus::Modified,
            added: 1,
            removed: 1,
        };
        let files = ["src/a.rs", "src/deep/b.rs", "srcs.rs"].map(row);
        let comments = ["src/a.rs", "src/a.rs", "src/deep/b.rs", "srcs.rs", "src/unchanged.rs"].map(comment);

        let counts = commented(&files, &comments);

        assert_eq!(counts.get("src/a.rs"), Some(&2));
        assert_eq!(counts.get("src/unchanged.rs"), None, "a file the branch doesn't change has no row");
        assert_eq!(in_folder(&counts, "src"), 3, "not srcs.rs, whose name only starts the same");
        assert_eq!(in_folder(&counts, "src/deep"), 1);
    }
}
