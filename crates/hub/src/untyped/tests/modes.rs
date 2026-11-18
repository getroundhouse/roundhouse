use protocol::capability::approvals::{ApprovalMode, Beyond, Sandbox, Unasked};
use protocol::event::EventBody;
use protocol::fixtures::{full_access, plan_mode};
use protocol::model::{AgentKind, SessionId};
use protocol::worker::WorkerFrame;
use serde_json::{Value, json};

use super::super::{Typing, naming_a_mode};

fn frame(body: &Value) -> String {
    json!({"frame":"agent_event","session_id":"s","run":1,"body":body}).to_string()
}

/// A worker of a build from before modes were typed names the mode its agent reports, which is typed by the kind of
/// the chat it's from, and only a frame that may name one is looked into for its chat.
#[test]
fn a_workers_frame_reads_with_the_mode_it_names_typed_by_its_chats_kind() {
    let read = |body: Value, kind| {
        let WorkerFrame::AgentEvent { body, .. } = Typing::default().frame(frame(&body).as_bytes(), kind).unwrap()
        else {
            panic!("not an agent's event")
        };
        body
    };
    let changed = json!({"type":"agent_mode","permission_mode":"plan"});
    let started = json!({"type":"agent_init","agent_session_id":"a","model":"m","cwd":"/w",
        "permission_mode":"bypassPermissions","agent_version":null});

    assert_eq!(naming_a_mode(frame(&changed).as_bytes()), Some(SessionId::from("s")));
    assert_eq!(naming_a_mode(frame(&json!({"type":"agent_mode","mode":plan_mode()})).as_bytes()), None);
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
