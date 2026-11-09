use super::*;
use crate::client::ClientRequest;
use crate::worker::WorkerRequest;

fn opus_at_max() -> SessionOptions {
    SessionOptions { model: Some("opus".into()), permission_mode: None, effort: Some(Effort::Max) }
}

/// Each request that carries the options, as a peer sent it before they were one value.
const BEFORE: [&str; 4] = [
    r#"{"method":"session_create","workspace_id":"w1","agent":"claude","model":"opus","permission_mode":null,"effort":"max","client_request_id":"r1"}"#,
    r#"{"method":"session_configure","id":"s1","model":"opus","permission_mode":null,"effort":"max"}"#,
    r#"{"method":"agent_configure","session_id":"s1","model":"opus","permission_mode":null,"effort":"max"}"#,
    r#"{"method":"agent_start","session_id":"s1","run":7,"agent":"claude","cwd":"/w","agent_session_id":"a1","resume":false,"model":"opus","permission_mode":null,"effort":"max","env":[]}"#,
];

/// A client's request starts `session_`, and anything else here is a worker's.
fn from_a_client(line: &str) -> bool {
    line.starts_with(r#"{"method":"session_"#)
}

fn options_of(line: &str) -> SessionOptions {
    if from_a_client(line) {
        let (ClientRequest::SessionCreate { options, .. } | ClientRequest::SessionConfigure { options, .. }) =
            serde_json::from_str::<ClientRequest>(line).unwrap()
        else {
            panic!("not a request with options: {line}");
        };
        return options;
    }
    let request = serde_json::from_str::<WorkerRequest>(line).unwrap();
    if let WorkerRequest::AgentStart(spec) = request {
        return spec.options;
    }
    let WorkerRequest::AgentConfigure { options, .. } = request else {
        panic!("not a request with options: {line}");
    };
    options
}

fn encoded_again(line: &str) -> String {
    if from_a_client(line) {
        serde_json::to_string(&serde_json::from_str::<ClientRequest>(line).unwrap()).unwrap()
    } else {
        serde_json::to_string(&serde_json::from_str::<WorkerRequest>(line).unwrap()).unwrap()
    }
}

#[test]
fn the_wire_form_is_what_it_was_before_the_options_were_one_value() {
    for line in BEFORE {
        assert_eq!(options_of(line), opus_at_max(), "{line}");

        assert_eq!(encoded_again(line), line);
    }
}

#[test]
fn a_request_from_before_effort_has_none() {
    let line = r#"{"method":"session_configure","id":"s1","model":null,"permission_mode":"plan"}"#;

    let options = options_of(line);

    assert_eq!(options, SessionOptions { permission_mode: Some("plan".into()), ..SessionOptions::default() });
}

#[test]
fn merging_takes_the_newer_setting_and_keeps_the_rest() {
    let older = opus_at_max();
    let newer = SessionOptions { model: None, permission_mode: Some("plan".into()), effort: Some(Effort::Low) };

    let merged = older.merged(newer);

    let expected =
        SessionOptions { model: Some("opus".into()), permission_mode: Some("plan".into()), effort: Some(Effort::Low) };
    assert_eq!(merged, expected);
}

#[test]
fn only_options_with_no_setting_are_empty() {
    assert!(SessionOptions::default().is_empty());
    assert!(!SessionOptions { effort: Some(Effort::Low), ..SessionOptions::default() }.is_empty());
}
