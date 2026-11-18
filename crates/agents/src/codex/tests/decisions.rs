use protocol::capability::approvals::Request;
use protocol::capability::asking::TypedAnswer;
use protocol::fixtures::{approval_request, asking, passed_decisions, plan, question, to_run};
use serde_json::json;

use crate::codex::approvals::can_answer;
use crate::codex::claims::claims;
use crate::codex::state::NativeRequest;

/// Every decision the hub passes on, on each kind of request Codex makes, is one this adapter tells it, so the hub
/// never records a decision Codex never hears.
#[test]
fn every_decision_the_hub_passes_on_reaches_codex() {
    let request = |asks| approval_request("asked", asks).request;
    let chosen = protocol::capability::asking::Question { typed: TypedAnswer::No, ..question("Which?", &["A"], false) };
    let asked: [(NativeRequest, Request); 4] = [
        (NativeRequest::Command(json!(1)), request(to_run("ls"))),
        (NativeRequest::File(json!(2)), request(to_run("ls"))),
        (
            NativeRequest::Questions { id: json!(3), keys: vec!["which".into(), "why".into()] },
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
