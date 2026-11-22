use protocol::capability::approvals::{ApprovalMode, Beyond, Sandbox, Unasked};
use protocol::event::EventBody;
use protocol::fixtures::{full_access, plan_mode};
use protocol::model::AgentKind;
use serde_json::{Value, json};

use super::super::type_mode;

/// An event recorded before modes were typed names the mode its agent reports, which is typed by the kind of the chat
/// it's from.
#[test]
fn an_event_reads_with_the_mode_it_names_typed_by_its_chats_kind() {
    let read = |mut body: Value, kind| {
        assert!(type_mode(&mut body, kind), "{body}");
        serde_json::from_value::<EventBody>(body).unwrap()
    };
    let changed = json!({"type":"agent_mode","permission_mode":"plan"});
    let started = json!({"type":"agent_init","agent_session_id":"a","model":"m","cwd":"/w",
        "permission_mode":"bypassPermissions","agent_version":null});

    assert!(!type_mode(&mut json!({"type":"agent_mode","mode":plan_mode()}), AgentKind::Claude), "already typed");
    assert_eq!(read(changed.clone(), AgentKind::Claude), EventBody::AgentMode { mode: plan_mode() });
    let codexs =
        ApprovalMode { unasked: Unasked::Reads, beyond: Beyond::Asks, sandbox: Some(Sandbox::ReadOnly), plan: true };
    assert_eq!(read(changed.clone(), AgentKind::Codex), EventBody::AgentMode { mode: codexs });
    assert_eq!(
        read(changed, AgentKind::Unknown),
        EventBody::AgentMode { mode: ApprovalMode::UNKNOWN },
        "no chat known"
    );
    let EventBody::AgentInit { mode, .. } = read(started, AgentKind::Claude) else { panic!("not an initialization") };
    assert_eq!(mode, Some(full_access()));
}
