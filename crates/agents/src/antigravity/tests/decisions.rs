use protocol::capability::approvals::Request;
use protocol::capability::asking::TypedAnswer;
use protocol::fixtures::{approval_request, asking, passed_decisions, plan, question, to_run};
use serde_json::json;

use crate::acp;
use crate::antigravity::approvals::can_answer;
use crate::antigravity::claims::claims;
use crate::antigravity::state::NativeRequest;

/// Every decision the hub passes on, on each kind of request Antigravity makes, is one this adapter tells it, so the
/// hub never records a decision Antigravity never hears.
#[test]
fn every_decision_the_hub_passes_on_reaches_antigravity() {
    let request = |asks| approval_request("asked", asks).request;
    let permission = acp::parse_permission_request(&json!({
        "toolCall": {"toolCallId": "call-1", "title": "Run command", "kind": "execute",
            "rawInput": {"CommandLine": "ls"}},
        "options": [{"optionId": "allow_once", "kind": "allow_once"},
            {"optionId": "reject_once", "kind": "reject_once"}],
    }))
    .expect("a request to make a call");
    let chosen = protocol::capability::asking::Question { typed: TypedAnswer::No, ..question("Which?", &["A"], false) };
    let asked: [(NativeRequest, Request); 3] = [
        (NativeRequest::Permission { id: json!(1), request: Box::new(permission) }, request(to_run("ls"))),
        (
            NativeRequest::Elicitation { id: json!(2), fields: Vec::new() },
            request(asking(vec![chosen, question("Why?", &[], false)])),
        ),
        (NativeRequest::Plan, request(plan("1. Read"))),
    ];

    for (native, request) in asked {
        let passed = passed_decisions(&request, &claims());
        assert!(!passed.is_empty(), "{:?} takes something", request.asks);
        for decision in passed {
            assert!(can_answer(&native, &decision), "{decision:?} on {:?}", request.asks);
        }
    }
}
