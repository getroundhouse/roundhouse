use protocol::model::WaitEnded;
use std::time::Duration;

use super::*;
use crate::tools::pull_request::tests::{checked, open_pr, with};

/// How often a test's CLI is told to ask after a wait.
const POLL: Duration = crate::Timers::DEFAULT.wait_poll;

/// A wait on [`open_pr`] that hasn't ended, for the tests of the tools that answer one.
pub(in crate::tools) fn waiting() -> Wait {
    Wait {
        id: "w1".into(),
        session_id: "s".into(),
        workspace_id: "v".into(),
        number: 605,
        call: Some("toolu_1".into()),
        until: WaitUntil::Done,
        watch: None,
        started_at_ms: 1_000,
        ended: None,
    }
}

pub(in crate::tools) fn ended(how: WaitEnd) -> Wait {
    Wait { ended: Some(WaitEnded { at_ms: 61_000, how }), ..waiting() }
}

#[test]
fn a_wait_still_waiting_is_a_working_task_saying_where_the_pull_request_stands() {
    let running = checked(open_pr(), 1, 0, 3);

    let task = task(&waiting(), Some(&running), Duration::from_millis(1_250));

    assert_eq!(task["resultType"], "complete");
    assert_eq!((&task["taskId"], &task["status"]), (&json!("w1"), &json!("working")));
    assert_eq!(task["statusMessage"], "Waiting on pull request #605. Its checks are still running: 1 of 4 are done.");
    assert_eq!((&task["createdAt"], &task["lastUpdatedAt"]), (&json!("1970-01-01T00:00:01.000Z"), &task["createdAt"]));
    assert_eq!((&task["ttlMs"], &task["pollIntervalMs"]), (&Value::Null, &json!(1_250)), "the poll it was given");
    assert!(task.get("result").is_none(), "{task}");
}

#[test]
fn an_ended_wait_is_a_completed_task_whose_result_says_how_and_where_the_pull_request_stands() {
    let merged = with(open_pr(), |said| said.state = protocol::model::PrState::Merged);

    let task = task(&ended(WaitEnd::Merged), Some(&merged), POLL);

    assert_eq!((&task["resultType"], &task["status"]), (&json!("complete"), &json!("completed")));
    assert_eq!(task["lastUpdatedAt"], "1970-01-01T00:01:01.000Z");
    let result = &task["result"];
    assert_eq!(result["isError"], false, "an ending the agent has to deal with is still the wait working");
    assert_eq!(
        result["content"][0]["text"],
        "The wait on pull request #605 has ended: it's merged.\n\nAs Roundhouse last read it:\n\
         Pull request #605, \"Wait on pull requests\", into main: https://github.com/o/r/pull/605\nIt's merged."
    );
}

/// The agent hears what the task says in a turn of its own, so it says who called it off, and whether to wait again.
#[test]
fn a_wait_called_off_is_a_cancelled_task_with_no_result_that_says_who_called_it_off() {
    let said = |how| {
        let task = task(&ended(how), Some(&open_pr()), POLL);
        assert_eq!(task["status"], "cancelled", "{how:?}");
        assert!(task.get("result").is_none(), "{task}");
        task["statusMessage"].as_str().unwrap().to_string()
    };

    assert_eq!(said(WaitEnd::Cancelled), "The wait was called off.");
    assert!(said(WaitEnd::CalledOffByPerson).contains("Don't start another unless they ask."));
    assert!(said(WaitEnd::Replaced).contains("replaced by the one started after it"));
}
