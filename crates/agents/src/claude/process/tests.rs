use std::os::unix::fs::PermissionsExt as _;

use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::capability::settings::Settings;
use protocol::model::{AgentKind, ModelName, RunId};

use super::recovery::{LaunchFailure, classify_launch_failure};
use super::*;
use crate::{BinaryLookup, Instructions};

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

fn spec(model: Option<&'static str>, mode: ApprovalMode) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: RunId::new(1),
        agent: AgentKind::Claude,
        cwd: "/tmp".into(),
        start: protocol::capability::conversation::Start::New { conversation: Some("abc".into()) },
        settings: Settings {
            model: Some(ModelName::named(model.unwrap_or("sonnet"))),
            mode: Some(mode),
            effort: Some(Effort::High),
            ..Settings::default()
        },
        env: vec![],
        tools_token: None,
        login: None,
        repository: None,
    }
}

fn args(spec: &AgentSpec, resume: bool) -> Vec<String> {
    build_args(spec, resume).unwrap()
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).map(|i| args[i + 1].clone())
}

#[test]
fn first_launch_names_the_conversation_and_later_ones_resume_it() {
    let first = args(&spec(None, crate::claude::claims::full_access()), false);
    assert_eq!(flag_value(&first, "--session-id").as_deref(), Some("abc"));
    assert!(!first.contains(&"--resume".to_string()));
    let later = args(&spec(None, crate::claude::claims::full_access()), true);
    assert_eq!(flag_value(&later, "--resume").as_deref(), Some("abc"));
    assert!(!later.contains(&"--session-id".to_string()));
}

/// A chat that's planning too, since a launch straight into plan mode would leave it for a mode that asks before every
/// tool, and a chat in a mode of the CLI's own, since a chat can't be given that.
#[test]
fn every_launch_has_full_access() {
    let full_access = spec(None, crate::claude::claims::full_access());
    let left_out = AgentSpec {
        settings: protocol::capability::settings::Settings { mode: None, ..full_access.settings.clone() },
        ..full_access.clone()
    };
    for (mode, spec) in
        [("full access", full_access), ("plan", spec(None, crate::claude::claims::plan())), ("none", left_out)]
    {
        let args = args(&spec, false);

        assert_eq!(flag_value(&args, "--permission-mode").as_deref(), Some("bypassPermissions"), "{mode:?}");
        assert!(args.iter().any(|a| a == "--allow-dangerously-skip-permissions"));
        assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }
}

#[test]
fn every_launch_names_its_model_and_effort() {
    for effort in crate::claude::description().efforts_for(&crate::claude::description::STARTS_ON.0) {
        let mut chosen = spec(Some("opus"), crate::claude::claims::full_access());
        chosen.settings.effort = Some(effort.clone());

        let launched = args(&chosen, true);

        assert_eq!(flag_value(&launched, "--model").as_deref(), Some("opus"));
        assert_eq!(flag_value(&launched, "--effort").as_deref(), crate::claude::effort::level(&effort));
    }
}

#[test]
fn the_agent_and_its_subagents_read_roundhouses_instructions_from_their_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("agents/upminster.s-1.instructions.txt");
    let told = Instructions { text: "Keep scratch in `.context/`.".into(), file: file.clone() };

    let given = instructions::args(&told).unwrap();

    let flags = ["--append-system-prompt-file", "--append-subagent-system-prompt-file"];
    assert_eq!(given, flags.map(|flag| [flag.into(), file.clone().into_os_string()]).concat());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Keep scratch in `.context/`.");
}

#[test]
fn a_level_this_build_cant_name_is_refused_rather_than_left_to_the_cli() {
    let mut unknown = spec(None, crate::claude::claims::full_access());
    unknown.settings.effort = Some(protocol::capability::settings::Effort::Unknown);

    assert!(matches!(build_args(&unknown, false), Err(AgentError::UnknownEffort)));
}

#[test]
fn approvals_always_come_back_to_the_host() {
    let args = args(&spec(None, crate::claude::claims::full_access()), false);
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

#[test]
fn only_a_refusal_of_a_request_the_cli_doesnt_know_says_asking_again_is_pointless() {
    let unknown = refused("Unsupported control request subtype: get_usage".to_string());
    let other = refused("Internal error".to_string());

    assert!(matches!(unknown, AgentError::UnknownRequest(_)), "{unknown:?}");
    assert!(matches!(other, AgentError::Refused(_)), "any other refusal may not come twice: {other:?}");
}
