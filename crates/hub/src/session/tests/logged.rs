//! What the log has of a session.

use super::super::Item;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
use logging::testing::Logs;
use protocol::capability::Report;
use protocol::model::RunId;

/// As when it's taken what its agent said, and as it's asked by a request.
#[tokio::test]
async fn what_a_sessions_tasks_log_says_whose_they_are() {
    let logs = Logs::capture();
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(1);
    let live = crate::spans::request(1, 2).in_scope(|| carried_on(&hub, &link, run));

    live.send_item(Item::Report { run, report: Report::Unknown, seq: None }).await;
    flushed(&live).await;

    let events = logs.events();
    let dropped =
        events.iter().find(|event| event["event"].as_str().unwrap_or_default().starts_with("session.report_"));
    let context = &dropped.unwrap_or_else(|| panic!("{}", logs.text()))["context"];
    assert_eq!(context["session_id"], "s", "{context}");
    assert_eq!(context["workspace_id"], live.workspace_id.as_str(), "{context}");
    assert!(context.get("call").is_none(), "nothing of the request it started in: {context}");
}
