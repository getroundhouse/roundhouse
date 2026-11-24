use protocol::event::EventBody;
use protocol::model::SessionId;

use super::super::tests::{notice, store_with_session};

fn started(build: Option<&str>) -> EventBody {
    EventBody::AgentStarted {
        pid: 1,
        resumed: true,
        build: build.map(str::to_string),
        previous_build: None,
        capabilities: None,
    }
}

#[test]
fn a_chat_s_last_build_is_its_newest_agent_s() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = SessionId::from("s");
    let append = |body: &EventBody| store.append(&id, None, body, None, None, None).unwrap();
    assert_eq!(store.last_agent_build(&id).unwrap(), None, "before any agent");

    append(&started(Some("0.9.0")));
    append(&notice("x"));
    assert_eq!(store.last_agent_build(&id).unwrap().as_deref(), Some("0.9.0"));
    append(&started(Some("1.0.0")));
    assert_eq!(store.last_agent_build(&id).unwrap().as_deref(), Some("1.0.0"));
    append(&started(None));
    assert_eq!(store.last_agent_build(&id).unwrap(), None, "a start that didn't say says nothing of the build");
}
