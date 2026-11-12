use protocol::model::WorkspaceStatus;

use super::*;
use crate::store::tests::store_with_session;
use crate::store::{InstanceStage, Move};

/// A workspace the last run left half-made is discarded as the hub starts, and the instance it was making is handed
/// over to be destroyed once the hub is serving.
#[test]
fn a_half_made_workspace_is_discarded_and_its_instance_handed_over_to_be_destroyed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut half_made = store.get_workspace(&"w".into()).unwrap();
    (half_made.id, half_made.name, half_made.instance_id) = ("birch".into(), "birch".into(), "birch-vm".into());
    half_made.status = WorkspaceStatus::Creating;
    store.insert_workspace(&half_made, None).unwrap();

    let unfinished = recover(&store).unwrap();

    let handed_over: Vec<_> = unfinished.iter().map(|instance| (instance.id.to_string(), instance.stage)).collect();
    assert_eq!(handed_over, [("birch-vm".to_string(), InstanceStage::Destroying)]);
    assert!(store.get_workspace(&"birch".into()).is_err(), "the workspace is discarded");
}

/// The last run can't say how far archiving or restoring got, so each workspace is as it was before, and can be
/// archived or restored again.
#[test]
fn a_workspace_left_being_archived_or_restored_is_put_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut restoring = store.get_workspace(&"w".into()).unwrap();
    (restoring.id, restoring.name, restoring.instance_id) = ("birch".into(), "birch".into(), "birch-vm".into());
    restoring.status = WorkspaceStatus::Restoring;
    store.insert_workspace(&restoring, None).unwrap();
    store.start_move(&"w".into(), Move::Archive).unwrap().unwrap();

    recover(&store).unwrap();

    assert_eq!(store.get_workspace(&"w".into()).unwrap().status, WorkspaceStatus::Ready);
    assert_eq!(store.get_workspace(&"birch".into()).unwrap().status, WorkspaceStatus::Archived);
}
