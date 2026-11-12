use protocol::model::{WorkspaceId, WorkspaceStatus};

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

    let handed_over: Vec<_> =
        unfinished.instances.iter().map(|instance| (instance.id.to_string(), instance.stage)).collect();
    assert_eq!(handed_over, [("birch-vm".to_string(), InstanceStage::Destroying)]);
    assert!(store.get_workspace(&"birch".into()).is_err(), "the workspace is discarded");
}

/// How far archiving or restoring got is the worker's to say, so each workspace's record goes on saying what the last
/// run was doing, and the hub is handed them to settle once it's serving.
#[test]
fn a_workspace_left_being_archived_or_restored_is_handed_over_as_it_was_left() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut restoring = store.get_workspace(&"w".into()).unwrap();
    (restoring.id, restoring.name, restoring.instance_id) = ("birch".into(), "birch".into(), "birch-vm".into());
    restoring.status = WorkspaceStatus::Restoring;
    store.insert_workspace(&restoring, None).unwrap();
    store.start_move(&"w".into(), Move::Archive).unwrap().unwrap();

    let left = recover(&store).unwrap();

    let (ready, archived) = (WorkspaceId::from("w"), WorkspaceId::from("birch"));
    assert_eq!(left.moves, [(ready, Move::Archive), (archived, Move::Restore)]);
    assert_eq!(store.get_workspace(&"w".into()).unwrap().status, WorkspaceStatus::Archiving);
    assert_eq!(store.get_workspace(&"birch".into()).unwrap().status, WorkspaceStatus::Restoring);
}

/// Only a worker this hub carries on with can be another build, and carrying on with one says which, so what the last
/// run recorded, as when it was killed while carrying on with one, says nothing of the workers there are now.
#[test]
fn a_starting_hub_forgets_which_build_each_worker_was() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_workspace_worker_build(&"w".into(), Some("0.9.0")).unwrap();

    recover(&store).unwrap();

    assert_eq!(store.get_workspace(&"w".into()).unwrap().worker_build, None);
}
