use protocol::model::Effort;

use super::*;

/// A session as a hub from before effort sends it, with no field for one.
const FROM_BEFORE_EFFORT: &str = r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus",
    "permission_mode":null,"status":"idle","last_seq":0,"created_at_ms":0,"updated_at_ms":0}"#;

fn from_before_effort() -> Session {
    serde_json::from_str(FROM_BEFORE_EFFORT).unwrap()
}

fn at(effort: Effort) -> SessionOptions {
    SessionOptions { effort: Some(effort), ..SessionOptions::default() }
}

#[test]
fn an_effort_a_hub_left_out_is_unsupported() {
    let session = from_before_effort();

    let result = as_asked(&at(Effort::Max), session);

    assert!(matches!(&result, Err(e) if e.is_unsupported()), "{result:?}");
}

#[test]
fn a_session_with_the_effort_asked_for_is_what_comes_back() {
    let session = Session { effort: Some(Effort::Max), ..from_before_effort() };

    let result = as_asked(&at(Effort::Max), session.clone());

    assert_eq!(result.unwrap(), session);
}

#[test]
fn a_hub_from_before_effort_still_sets_the_rest() {
    let model = SessionOptions { model: Some("opus".into()), ..SessionOptions::default() };

    let result = as_asked(&model, from_before_effort());

    assert_eq!(result.unwrap().model.as_deref(), Some("opus"));
}
