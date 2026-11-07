use std::os::unix::fs::PermissionsExt as _;

use protocol::model::AgentKind;

use super::supervise::{LaunchFailure, classify_launch_failure};
use super::*;
use crate::BinaryLookup;

fn executable(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_named_binary_is_used_without_looking_anywhere_else() {
    let lookup = BinaryLookup { explicit: Some("/nonexistent/claude".into()), path: None, home: None };

    assert_eq!(resolve_binary(&lookup), Some(PathBuf::from("/nonexistent/claude")));
}

#[test]
fn the_path_is_searched_before_the_places_installers_use() {
    let dir = tempfile::tempdir().unwrap();
    let on_path = dir.path().join("bin/claude");
    let installed = dir.path().join("home/.local/bin/claude");
    executable(&installed);
    let home = Some(dir.path().join("home"));
    let path = Some(dir.path().join("bin").into_os_string());

    let lookup = BinaryLookup { explicit: None, path: path.clone(), home: home.clone() };
    assert_eq!(resolve_binary(&lookup), Some(installed), "nothing on the path yet");

    executable(&on_path);
    let lookup = BinaryLookup { explicit: None, path, home };
    assert_eq!(resolve_binary(&lookup), Some(on_path));
}

#[test]
fn a_file_that_cant_be_run_is_not_the_binary() {
    let dir = tempfile::tempdir().unwrap();
    let not_runnable = dir.path().join("claude");
    std::fs::write(&not_runnable, "").unwrap();
    let path = Some(dir.path().as_os_str().to_owned());

    let lookup = BinaryLookup { explicit: None, path, home: None };

    assert_ne!(resolve_binary(&lookup), Some(not_runnable));
}

fn spec(model: Option<&str>, mode: Option<&str>) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: 1,
        agent: AgentKind::Claude,
        cwd: "/tmp".into(),
        agent_session_id: "abc".into(),
        resume: false,
        model: model.map(String::from),
        permission_mode: mode.map(String::from),
        env: vec![],
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).map(|i| args[i + 1].clone())
}

#[test]
fn first_launch_names_the_conversation_and_later_ones_resume_it() {
    let first = build_args(&spec(None, None), false);
    assert_eq!(flag_value(&first, "--session-id").as_deref(), Some("abc"));
    assert!(!first.contains(&"--resume".to_string()));
    let later = build_args(&spec(None, None), true);
    assert_eq!(flag_value(&later, "--resume").as_deref(), Some("abc"));
    assert!(!later.contains(&"--session-id".to_string()));
}

#[test]
fn with_no_mode_the_users_own_agent_settings_decide() {
    assert!(!build_args(&spec(None, None), false).contains(&"--permission-mode".to_string()));
    let args = build_args(&spec(Some("haiku"), Some("manual")), false);
    assert_eq!(flag_value(&args, "--permission-mode").as_deref(), Some("manual"));
    assert_eq!(flag_value(&args, "--model").as_deref(), Some("haiku"));
}

#[test]
fn full_access_is_available_but_never_on_by_itself() {
    let args = build_args(&spec(None, None), false);
    assert!(args.iter().any(|a| a == "--allow-dangerously-skip-permissions"));
    assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
    assert_eq!(flag_value(&args, "--permission-mode"), None);
}

#[test]
fn the_default_model_has_no_flag() {
    let mut default = spec(None, None);
    default.model = Some("default".to_string());
    assert_eq!(flag_value(&build_args(&default, false), "--model"), None);
    let mut chosen = spec(None, None);
    chosen.model = Some("opus".to_string());
    assert_eq!(flag_value(&build_args(&chosen, false), "--model").as_deref(), Some("opus"));
}

#[test]
fn approvals_always_come_back_to_the_host() {
    let args = build_args(&spec(None, None), false);
    assert_eq!(flag_value(&args, "--permission-prompt-tool").as_deref(), Some("stdio"));
}

#[test]
fn launch_failures_are_told_apart_from_the_clis_messages() {
    let line = |s: &str| vec![s.to_string()];
    assert_eq!(
        classify_launch_failure(&line("No conversation found with session ID: abc")),
        Some(LaunchFailure::NoConversation)
    );
    assert_eq!(
        classify_launch_failure(&line("Error: Session ID abc is already in use.")),
        Some(LaunchFailure::AlreadyExists)
    );
    assert_eq!(classify_launch_failure(&line("Error: something else")), None);
}
