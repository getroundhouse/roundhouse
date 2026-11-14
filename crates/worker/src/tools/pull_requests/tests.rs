use super::*;
use protocol::fixtures::pull_request_wait;
use protocol::model::{WaitEnd, WaitEnded};

#[test]
fn invalid_or_ambiguous_wait_arguments_are_refused_before_asking_the_hub() {
    for input in [
        json!({}),
        json!({"pull_requests":[],"until":"merged"}),
        json!({"pull_requests":[1,1],"until":"merged"}),
        json!({"pull_requests":[0],"until":"merged"}),
        json!({"pull_requests":[-1],"until":"merged"}),
        json!({"pull_requests":[1],"until":"checks"}),
        json!({"pull_requests":[1],"until":"merged","timeout_seconds":0}),
        json!({"pull_requests":[1],"until":"merged","timeout_seconds":null}),
        json!({"pull_requests":[1],"until":"merged","extra":true}),
    ] {
        assert!(arguments(json!({"arguments":input}).as_object().unwrap()).is_err(), "{input}");
    }
    let parsed = arguments(
        json!({"arguments":{"pull_requests":[9,2],"until":"required_checks"},
        "_meta":{"claudecode/toolUseId":"call"}})
        .as_object()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(parsed.pull_requests.numbers().iter().map(|n| n.get()).collect::<Vec<_>>(), [2, 9]);
    assert_eq!(parsed.timeout_seconds, None);
    assert_eq!(parsed.call.unwrap().as_str(), "call");
}

#[test]
fn reads_reject_arguments_their_schema_does_not_allow() {
    assert!(read_arguments(json!({"arguments":{"pull_requests":[7],"until":"merged"}}).as_object().unwrap()).is_err());
}

#[test]
fn terminal_tasks_include_the_frozen_structured_result_and_human_stop_instruction() {
    let wait = Wait { ended: Some(WaitEnded { at_ms: 9, how: WaitEnd::CalledOffByPerson }), ..pull_request_wait("a") };

    let task = super::super::wait::task(&wait, None);

    assert_eq!(task["status"], "cancelled");
    assert!(task["statusMessage"].as_str().unwrap().contains("Don't start another"));
    assert_eq!(task["result"]["structuredContent"]["outcome"], "cancelled");
    assert_eq!(task["result"]["structuredContent"]["watch"]["snapshot"]["observed_at_ms"], 1);
}

#[test]
fn required_check_success_does_not_claim_optional_checks_passed() {
    let mut wait = Wait { ended: Some(WaitEnded { at_ms: 9, how: WaitEnd::ChecksPassed }), ..pull_request_wait("a") };
    let row = &mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0];
    row.checks[0].required = false;
    row.checks[0].state = CheckState::Failed;

    let task = super::super::wait::task(&wait, None);

    assert!(task["statusMessage"].as_str().unwrap().contains("required checks are satisfied"));
    let text = task["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("required checks are satisfied"));
    assert!(!text.contains("none failed"));
}
