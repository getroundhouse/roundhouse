//! A workspace's status, which a hub keeps in its database in the form it sends, and its branches and its worker's
//! build, which a hub sends as observations.

use protocol::client::HubNotice;
use protocol::frame;
use protocol::model::{
    Branch, BranchName, Branches, CommitId, Head, Observation, ObservationKey, Observed, OtherBuild, Rev,
    WorkspaceStatus,
};

use crate::keeps_its_form;

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

/// A workspace's branches, which a hub sends at the workspace's level to a client of a version that reads them so, as a
/// notice and as their removal.
#[test]
fn a_workspaces_branches_keep_their_form_as_a_notice_and_a_removal() {
    let branch = Branch { name: BranchName::from("aspen"), tip: CommitId::from("c1"), upstream: None };
    let branches = Branches { head: Head::Branch { name: BranchName::from("aspen") }, had: vec![branch] };
    let branches = Observed::Current { value: branches };
    let copy =
        HubNotice::Observation(Observation::WorkspaceBranches { workspace_id: "w".into(), branches, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WorkspaceBranches { workspace_id: "w".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        r#"{"notice":"observation","kind":"workspace_branches","workspace_id":"w","branches":{"freshness":"current","value":{"head":{"kind":"branch","name":"aspen"},"had":[{"name":"aspen","tip":"c1"}]}},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"workspace_branches","workspace_id":"w"},"rev":6}"#,
    );
}

/// That a workspace's worker is another build, which a hub sends at its own level to a client of a version that reads it
/// so, as a notice, of a worker that named its build and one too old to, and as its removal.
#[test]
fn a_workspaces_worker_build_keeps_its_form_as_a_notice_and_a_removal() {
    let build = |said: Option<&str>| Observed::Current { value: OtherBuild { said: said.map(str::to_string) } };
    let copy = |said| {
        HubNotice::Observation(Observation::WorkspaceWorkerBuild {
            workspace_id: "w".into(),
            build: build(said),
            rev: Rev::new(5),
        })
    };
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WorkspaceWorkerBuild { workspace_id: "w".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy(Some("0.9.0")),
        r#"{"notice":"observation","kind":"workspace_worker_build","workspace_id":"w","build":{"freshness":"current","value":{"said":"0.9.0"}},"rev":5}"#,
    );
    keeps_its_form(
        &copy(None),
        r#"{"notice":"observation","kind":"workspace_worker_build","workspace_id":"w","build":{"freshness":"current","value":{}},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"workspace_worker_build","workspace_id":"w"},"rev":6}"#,
    );
}
