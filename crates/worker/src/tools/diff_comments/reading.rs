use super::super::mcp::Called;
use crate::asks::Asks;
use protocol::model::{DiffComment, SessionId};
use protocol::worker::{DiffComments, ReadDiffComments};
use serde_json::{Value, json};

pub(in super::super) const NAME: &str = "get_diff_comments";

const DESCRIPTION: &str = "Reads the review comments the person has left on lines of this workspace's diff in \
    Roundhouse, oldest first: each one's file and line, whether that line is one the diff removes, and what it says. \
    The diff is this workspace's changes against its base. A comment stays until the person deletes it, so one you've \
    already addressed can still be there. Use this when the person mentions their comments on the diff, or asks you to \
    address them.";

pub(in super::super) fn definition() -> Value {
    json!({
        "name": NAME,
        "title": "Get the diff's comments",
        "description": DESCRIPTION,
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        "annotations": { "readOnlyHint": true, "openWorldHint": false },
    })
}

pub(in super::super) async fn call(asks: &Asks, session_id: &SessionId) -> Called {
    match asks.ask(session_id.clone(), ReadDiffComments).await {
        Ok(DiffComments { comments }) => Called::text(describe(&comments)),
        Err(e) => Called::failed(format!("Roundhouse couldn't read the diff's comments: {}", e.message)),
    }
}

/// The comments in words, each on a line of its own as Add to chat puts one in the composer, with any lines it goes on
/// to indented under it.
pub(super) fn describe(comments: &[DiffComment]) -> String {
    let count = match comments.len() {
        0 => return "There are no comments on this workspace's diff.".to_string(),
        1 => "1 comment".to_string(),
        n => format!("{n} comments"),
    };
    let mut lines = vec![format!("{count} on this workspace's diff, oldest first:")];
    lines.extend(comments.iter().map(|comment| format!("- {}", comment.for_agent().replace('\n', "\n  "))));
    lines.join("\n")
}
