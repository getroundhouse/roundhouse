use protocol::model::{PullRequestSelection, Session};
use protocol::worker::WorkerAsk;
use serde_json::json;

#[test]
fn plural_waits_are_authoritative_but_omitted_waits_keep_the_legacy_projection() {
    let mut value = serde_json::to_value(protocol::fixtures::session("s")).unwrap();
    value["wait"] = serde_json::to_value(protocol::fixtures::pull_request_wait("a")).unwrap();
    let old: Session = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(old.open_waits().len(), 1);
    value["waits"] = json!([]);
    let new: Session = serde_json::from_value(value).unwrap();
    assert!(new.open_waits().is_empty());
}

#[test]
fn explicit_wait_instructions_refuse_invalid_conditions_and_selections() {
    for input in [json!([]), json!([0]), json!([1, 1]), json!([-1]), json!([2_147_483_648_u64])] {
        assert!(serde_json::from_value::<PullRequestSelection>(input).is_err());
    }
    let input = json!({"method":"start_pull_request_wait","pull_requests":[7],"until":"future_condition"});
    // Unknown instructions are rejected by the envelope, never reinterpreted as a legacy wait.
    assert!(serde_json::from_value::<WorkerAsk>(input).is_err());
}
