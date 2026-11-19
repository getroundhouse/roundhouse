//! What a worker asks its hub about the comments on the workspace's diff, for an agent's tool, and the hub's answer.

use protocol::model::{DiffComment, DiffSide, Rev};
use protocol::worker::{Ask as _, AskReply, DiffComments, ReadDiffComments, WorkerAsk};

use super::keeps_its_form;

#[test]
fn reading_the_diffs_comments_keeps_its_form() {
    let comment = DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::Old,
        line: 3,
        text: "why?".into(),
        created_at_ms: 1,
        rev: Rev::new(2),
    };
    let fields =
        r#""id":"c","workspace_id":"w","path":"a.rs","side":"old","line":3,"text":"why?","created_at_ms":1,"rev":2"#;

    keeps_its_form(&ReadDiffComments.into_ask(), r#"{"method":"diff_comments"}"#);
    let newer: WorkerAsk = serde_json::from_str(r#"{"method":"diff_comments","newer":1}"#).unwrap();
    assert_eq!(newer, ReadDiffComments.into_ask(), "an ask with no fields reads with one a newer worker added");
    keeps_its_form(
        &AskReply::DiffComments(DiffComments { comments: vec![comment] }),
        &format!(r#"{{"result":"diff_comments","comments":[{{{fields}}}]}}"#),
    );
}
