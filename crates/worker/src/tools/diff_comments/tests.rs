use protocol::model::{DiffComment, DiffSide, Rev};

use super::*;

fn comment(path: &str, side: DiffSide, line: u32, text: &str) -> DiffComment {
    DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: path.into(),
        side,
        line,
        text: text.into(),
        created_at_ms: 0,
        rev: Rev::default(),
    }
}

#[test]
fn no_comments_is_said_rather_than_left_empty() {
    assert_eq!(describe(&[]), "There are no comments on this workspace's diff.");
}

#[test]
fn each_comment_is_a_line_that_says_where_it_is_and_what_it_says() {
    let text = describe(&[
        comment("src/a.rs", DiffSide::New, 12, "Rename this"),
        comment("src/b.rs", DiffSide::Old, 3, "Why remove this?\nIt was used."),
    ]);

    assert_eq!(
        text,
        "2 comments on this workspace's diff, oldest first:\n\
         - `src/a.rs:12`: Rename this\n\
         - `src/b.rs:3` (a removed line): Why remove this?\n  It was used."
    );
    assert!(describe(&[comment("a.rs", DiffSide::New, 1, "Why?")]).starts_with("1 comment on"), "one, not 1 comments");
}
