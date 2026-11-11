use super::*;
use crate::client::ClientRequest;
use crate::model::Session;
use crate::worker::WorkerRequest;

const OPUS: ModelName = ModelName::named("opus");

fn opus_at_max() -> SessionSettingsChange {
    SessionSettingsChange { model: Some(OPUS), permission_mode: None, effort: Some(Effort::Max) }
}

/// Each request that changes the settings, with each setting a field of its own.
const CHANGES: [&str; 3] = [
    r#"{"method":"session_create","workspace_id":"w1","agent":"claude","model":"opus","permission_mode":null,"effort":"max","client_request_id":"r1"}"#,
    r#"{"method":"session_configure","id":"s1","model":"opus","permission_mode":null,"effort":"max"}"#,
    r#"{"method":"agent_configure","session_id":"s1","model":"opus","permission_mode":null,"effort":"max"}"#,
];

const LAUNCH: &str = r#"{"method":"agent_start","session_id":"s1","run":7,"agent":"claude","cwd":"/w","agent_session_id":"a1","resume":false,"model":"opus","permission_mode":null,"effort":"max","env":[]}"#;

/// A client's request starts `session_`, and anything else here is a worker's.
fn from_a_client(line: &str) -> bool {
    line.starts_with(r#"{"method":"session_"#)
}

fn change_of(line: &str) -> SessionSettingsChange {
    if from_a_client(line) {
        let (ClientRequest::SessionCreate { options, .. } | ClientRequest::SessionConfigure { options, .. }) =
            serde_json::from_str::<ClientRequest>(line).unwrap()
        else {
            panic!("not a request with options: {line}");
        };
        return options;
    }
    let WorkerRequest::AgentConfigure { options, .. } = serde_json::from_str::<WorkerRequest>(line).unwrap() else {
        panic!("not a request with options: {line}");
    };
    options
}

fn decodes(line: &str) -> bool {
    if from_a_client(line) {
        serde_json::from_str::<ClientRequest>(line).is_ok()
    } else {
        serde_json::from_str::<WorkerRequest>(line).is_ok()
    }
}

fn encoded_again(line: &str) -> String {
    if from_a_client(line) {
        serde_json::to_string(&serde_json::from_str::<ClientRequest>(line).unwrap()).unwrap()
    } else {
        serde_json::to_string(&serde_json::from_str::<WorkerRequest>(line).unwrap()).unwrap()
    }
}

#[test]
fn on_the_wire_each_setting_is_a_field_of_its_own() {
    for line in CHANGES {
        assert_eq!(change_of(line), opus_at_max(), "{line}");

        assert_eq!(encoded_again(line), line);
    }
    let WorkerRequest::AgentStart(spec) = serde_json::from_str(LAUNCH).unwrap() else { panic!("not a launch") };
    assert_eq!(spec.settings, SessionSettings { model: OPUS, permission_mode: None, effort: Effort::Max });
    assert_eq!(encoded_again(LAUNCH), LAUNCH);
}

#[test]
fn a_model_that_would_leave_the_choice_to_the_cli_doesnt_read() {
    for line in CHANGES.into_iter().chain([LAUNCH]) {
        for refused in [r#""model":"default""#, r#""model":"""#] {
            let line = line.replace(r#""model":"opus""#, refused);

            assert!(!decodes(&line), "{line}");
        }
    }
}

#[test]
fn a_launch_always_names_its_model_and_effort() {
    for left_out in [r#""model":"opus","#, r#","effort":"max""#] {
        assert!(!decodes(&LAUNCH.replace(left_out, "")), "{left_out}");
    }
    for null in [(r#""model":"opus""#, r#""model":null"#), (r#""effort":"max""#, r#""effort":null"#)] {
        assert!(!decodes(&LAUNCH.replace(null.0, null.1)), "{}", null.1);
    }
}

#[test]
fn a_session_always_names_its_model_and_effort() {
    let session = r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus","permission_mode":null,
        "effort":"xhigh","status":"idle","last_seq":0,"created_at_ms":0,"updated_at_ms":0}"#;
    assert_eq!(serde_json::from_str::<Session>(session).unwrap().settings(), SessionSettings::default());

    for (named, left) in
        [(r#""model":"opus","#, ""), (r#""effort":"xhigh","#, ""), (r#""model":"opus""#, r#""model":null"#)]
    {
        assert!(serde_json::from_str::<Session>(&session.replace(named, left)).is_err(), "{named} to {left:?}");
    }
}

#[test]
fn a_change_from_before_effort_has_none() {
    let line = r#"{"method":"session_configure","id":"s1","model":null,"permission_mode":"plan"}"#;

    let change = change_of(line);

    assert_eq!(
        change,
        SessionSettingsChange { permission_mode: Some("plan".into()), ..SessionSettingsChange::default() }
    );
}

#[test]
fn a_change_takes_what_it_sets_and_keeps_the_rest() {
    let before = SessionSettings { model: OPUS, permission_mode: Some("acceptEdits".into()), effort: Effort::Max };
    let change = SessionSettingsChange { model: None, permission_mode: Some("plan".into()), effort: Some(Effort::Low) };

    let after = before.changed(change);

    assert_eq!(after, SessionSettings { model: OPUS, permission_mode: Some("plan".into()), effort: Effort::Low });
}

#[test]
fn a_session_nobody_has_chosen_for_runs_the_defaults_in_the_users_own_mode() {
    assert_eq!(
        SessionSettings::default(),
        SessionSettings { model: DEFAULT_MODEL, permission_mode: None, effort: DEFAULT_EFFORT }
    );
    assert_eq!(SessionSettings::default().changed(SessionSettingsChange::default()), SessionSettings::default());
}
