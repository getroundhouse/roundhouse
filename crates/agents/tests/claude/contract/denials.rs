//! Which results say their call was denied, in the capture that denies one request and stops a turn while each of two
//! more waits.

use protocol::event::{ApprovalOutcome, ContentBlock, Denied, EventBody};
use protocol::model::{ApprovalId, ToolUseId};

use super::{blocks, events};

#[test]
fn only_the_call_the_person_denied_says_so_and_not_those_a_stop_cancelled() {
    let events = events();
    let cancelled: Vec<&ApprovalId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled, .. } => Some(approval_id),
            _ => None,
        })
        .collect();
    let stopped: Vec<&ToolUseId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalRequested { approval_id, tool_use_id: Some(call), .. }
                if cancelled.contains(&approval_id) =>
            {
                Some(call)
            }
            _ => None,
        })
        .collect();
    assert_eq!(stopped.len(), 2, "the capture stops a turn while each of two requests waits");

    let results: Vec<(ToolUseId, Option<Denied>)> = blocks()
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, denied, .. } => Some((tool_use_id, denied)),
            _ => None,
        })
        .collect();
    for call in stopped {
        let (_, denied) = results.iter().find(|(id, _)| id == call).expect("a stopped call has a result");
        assert_eq!(denied, &None, "{call:?} was stopped, not denied");
    }
    let denied: Vec<&Denied> = results.iter().filter_map(|(_, denied)| denied.as_ref()).collect();
    assert_eq!(denied, [&Denied { said: Some("Not allowed".into()) }], "the one the person denied, in their words");
}
