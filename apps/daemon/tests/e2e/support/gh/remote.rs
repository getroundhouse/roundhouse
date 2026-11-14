//! A complete synthetic response for explicit PR reads. It follows the state set by `StandInGh::say`.

use serde_json::Value;

pub(super) fn response(pr: &Value) -> Value {
    let mut response: Value =
        serde_json::from_str(include_str!("../../../../../../crates/worker/tests/fixtures/remote_pull_requests.json"))
            .unwrap();
    response["data"]["repository"]["pullRequest"].as_object_mut().unwrap().extend(pr.as_object().unwrap().clone());
    response
}
