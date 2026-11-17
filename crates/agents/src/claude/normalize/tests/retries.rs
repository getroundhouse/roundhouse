//! The CLI trying a failed request to its model again, which is live, as the session's tail holds it.

use super::super::*;
use protocol::tail::Retrying;
use serde_json::json;

/// The CLI says before each try of a request that failed which try comes next, of how many, the status the last failed
/// with and how long it waits, which is live, until the agent writes again. One with no try is kept for a later build.
#[test]
fn a_request_tried_again_is_live() {
    let line = |more: Value| {
        let mut line = json!({"type":"system","subtype":"api_retry","max_retries":10,"retry_delay_ms":1_204,
            "error":"server_error","session_id":"s","uuid":"u"});
        line.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        line.to_string()
    };

    assert_eq!(
        parse_line(&line(json!({"attempt":3,"error_status":503}))),
        Parsed::Retrying(Retrying { run: None, attempt: 3, of: Some(10), status: Some(503), delay_ms: Some(1_204) })
    );
    let Parsed::Retrying(unanswered) = parse_line(&line(json!({"attempt":1,"error_status":null}))) else { panic!() };
    assert_eq!(unanswered.status, None, "a try that got no answer");
    assert_eq!(
        parse_line(&line(json!({}))),
        Parsed::Event(EventBody::AgentOther { kind: "system/api_retry".into(), subagent: None })
    );
}
