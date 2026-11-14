use super::*;
use crate::capability::approvals::{ApprovalMode, Beyond, Unasked};
use crate::capability::settings;
use crate::client::{ClientRequest, sessions};
use crate::model::{NotOffered, Session};
use crate::worker::WorkerRequest;

const OPUS: ModelName = ModelName::named("opus");

fn opus_at_max() -> SessionSettingsChange {
    SessionSettingsChange { model: Some(OPUS), permission_mode: None, effort: Some(Effort::Max) }
}

/// Each request of a client's that changes the settings, with each setting a field of its own.
const CHANGES: [&str; 2] = [
    r#"{"method":"session_create","workspace_id":"w1","agent":"claude","model":"opus","permission_mode":null,"effort":"max","client_request_id":"r1"}"#,
    r#"{"method":"session_configure","id":"s1","model":"opus","permission_mode":null,"effort":"max"}"#,
];

const LAUNCH: &str = r#"{"method":"agent_start","session_id":"s1","run":7,"agent":"claude","cwd":"/w","start":"new","conversation":"a1","settings":{"model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"max","speed":null,"thinking":null,"persona":null,"style":null,"verbosity":null,"advisor":null,"budget":null},"env":[]}"#;

/// A client's request starts `session_`, and anything else here is a worker's.
fn from_a_client(line: &str) -> bool {
    line.starts_with(r#"{"method":"session_"#)
}

const FULL_ACCESS: ApprovalMode =
    ApprovalMode { unasked: Unasked::Everything, beyond: Beyond::Asks, sandbox: None, plan: false };

fn change_of(line: &str) -> SessionSettingsChange {
    if from_a_client(line) {
        let (ClientRequest::SessionCreate(sessions::SessionCreate { options, .. })
        | ClientRequest::SessionConfigure(sessions::SessionConfigure { options, .. })) =
            serde_json::from_str::<ClientRequest>(line).unwrap()
        else {
            panic!("not a request with options: {line}");
        };
        return options;
    }
    panic!("not a request with options: {line}");
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
    let typed = settings::Settings {
        model: Some(OPUS),
        mode: Some(FULL_ACCESS),
        effort: Some(settings::Effort::Max),
        ..settings::Settings::default()
    };
    assert_eq!(spec.settings, typed);
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
fn an_adapter_runs_a_launch_with_its_model_and_only_settings_it_can_name() {
    let defaults = SessionSettings { model: OPUS, permission_mode: PermissionMode::Plan, effort: Effort::Low };
    let typed = settings::Settings {
        model: Some(OPUS),
        mode: Some(FULL_ACCESS),
        effort: Some(settings::Effort::Max),
        ..settings::Settings::default()
    };
    let named = SessionSettings { model: OPUS, permission_mode: PermissionMode::FullAccess, effort: Effort::Max };
    assert_eq!(SessionSettings::from_typed(&typed, &defaults), Ok(named));
    let planning = settings::Settings { mode: Some(ApprovalMode { plan: true, ..FULL_ACCESS }), ..typed.clone() };
    assert_eq!(SessionSettings::from_typed(&planning, &defaults).unwrap().permission_mode, PermissionMode::Plan);
    let only_a_model = settings::Settings { model: Some(OPUS), ..settings::Settings::default() };
    assert_eq!(SessionSettings::from_typed(&only_a_model, &defaults), Ok(defaults.clone()), "the rest is the kind's");

    let unnamed = settings::Settings { model: None, ..typed.clone() };
    assert_eq!(SessionSettings::from_typed(&unnamed, &defaults), Err(NotOffered::Settings));
    let newer_effort = settings::Settings { effort: Some(settings::Effort::Unknown), ..typed.clone() };
    assert_eq!(SessionSettings::from_typed(&newer_effort, &defaults), Err(NotOffered::Effort));
    let unnamed_effort = settings::Settings { effort: Some(settings::Effort::Persistent), ..typed.clone() };
    assert_eq!(SessionSettings::from_typed(&unnamed_effort, &defaults), Err(NotOffered::Effort));
    let newer_mode =
        settings::Settings { mode: Some(ApprovalMode { unasked: Unasked::Unknown, ..FULL_ACCESS }), ..typed };
    assert_eq!(SessionSettings::from_typed(&newer_mode, &defaults), Err(NotOffered::Mode));
}

#[test]
fn a_session_always_names_its_model_mode_and_effort() {
    let session = r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus","permission_mode":"bypassPermissions",
        "effort":"xhigh","status":"idle","last_seq":0,"created_at_ms":0,"updated_at_ms":0}"#;
    let named = SessionSettings { model: OPUS, permission_mode: PermissionMode::FullAccess, effort: Effort::Xhigh };
    assert_eq!(serde_json::from_str::<Session>(session).unwrap().settings(), named);

    for (named, left) in [
        (r#""model":"opus","#, ""),
        (r#""permission_mode":"bypassPermissions","#, ""),
        (r#""effort":"xhigh","#, ""),
        (r#""model":"opus""#, r#""model":null"#),
        (r#""permission_mode":"bypassPermissions""#, r#""permission_mode":null"#),
    ] {
        assert!(serde_json::from_str::<Session>(&session.replace(named, left)).is_err(), "{named} to {left:?}");
    }
}

#[test]
fn a_change_from_before_effort_has_none() {
    let line = r#"{"method":"session_configure","id":"s1","model":null,"permission_mode":"plan"}"#;

    let change = change_of(line);

    assert_eq!(
        change,
        SessionSettingsChange { permission_mode: Some(PermissionMode::Plan), ..SessionSettingsChange::default() }
    );
}

#[test]
fn a_change_takes_what_it_sets_and_keeps_the_rest() {
    let before = SessionSettings { model: OPUS, permission_mode: PermissionMode::FullAccess, effort: Effort::Max };
    let change =
        SessionSettingsChange { model: None, permission_mode: Some(PermissionMode::Plan), effort: Some(Effort::Low) };

    let after = before.changed(change);

    assert_eq!(after, SessionSettings { model: OPUS, permission_mode: PermissionMode::Plan, effort: Effort::Low });
}
