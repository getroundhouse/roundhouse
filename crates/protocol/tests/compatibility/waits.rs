use protocol::model::{PullRequestSelection, PullRequestWatch, Session};
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

/// A wait's pull requests go out as `watching`, in their facets, and one a hub from before sent as `watch`, in a form
/// this client doesn't read, reads as a wait without them, rather than failing its chat's record.
#[test]
fn a_waits_pull_requests_go_by_a_name_a_client_from_before_skips() {
    let mut wait = protocol::fixtures::pull_request_wait("a");
    wait.watch.as_mut().unwrap().snapshot.pull_requests = vec![super::github_shown::every_facet()];

    let mut written = serde_json::to_value(&wait).unwrap();
    assert!(written.as_object_mut().unwrap().remove("watching").is_some(), "sent as `watching`");
    written["watch"] = json!({"snapshot": {"pull_requests": [{"pull_request": {"number": 7}}]}});
    let older: protocol::model::Wait = serde_json::from_value(written).unwrap();

    // As it's written, and as the store keeps it, in the `watch` column of `waits`.
    let every_facet = super::github_shown::EVERY_FACET;
    let kept = format!(
        r#"{{"requested":[7],"snapshot":{{"repository":{{"url":"https://github.com/example/project","name_with_owner":"example/project"}},"observed_at_ms":1,"pull_requests":[{every_facet}]}},"baseline":[],"deadline_at_ms":null,"read_error":null,"last_attempt_at_ms":1,"affected":[]}}"#
    );
    assert_eq!(serde_json::to_string(wait.watch.as_deref().unwrap()).unwrap(), kept);
    assert_eq!(serde_json::from_str::<PullRequestWatch>(&kept).unwrap(), **wait.watch.as_ref().unwrap());
    assert_eq!(older.watch, None);
}
