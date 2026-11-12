//! A workspace's status, which a hub keeps in its database in the form it sends.

use protocol::client::HubNotice;
use protocol::frame;
use protocol::model::WorkspaceStatus;

/// A hub stores a workspace's status as the text it sends, so a workspace being archived or restored, which a hub that
/// stopped part way leaves stored, has to read the same way to the next.
#[test]
fn a_workspace_on_its_way_out_or_back_keeps_its_form() {
    let workspace = |status: &str| {
        format!(
            r#"{{"notice":"workspace","id":"w","repo_id":"r","name":"n","branch":"n","base_ref":null,"path":"/w/n","status":"{status}","provider":"local","instance_id":"w","created_at_ms":1,"archived_at_ms":null,"setup":{{"state":"succeeded"}},"port_base":55000}}"#
        )
    };
    let read = |status: &str| {
        let notice: HubNotice = frame::decode(workspace(status).as_bytes()).unwrap();
        let HubNotice::Workspace(workspace) = notice else { panic!("{notice:?}") };
        workspace.status
    };

    assert_eq!(read("archiving"), WorkspaceStatus::Archiving);
    assert_eq!(read("restoring"), WorkspaceStatus::Restoring);
    assert_eq!(serde_json::to_string(&WorkspaceStatus::Archiving).unwrap(), r#""archiving""#);
    assert_eq!(serde_json::to_string(&WorkspaceStatus::Restoring).unwrap(), r#""restoring""#);
}
