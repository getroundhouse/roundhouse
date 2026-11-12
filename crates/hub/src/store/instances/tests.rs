use protocol::model::{Workspace, WorkspaceStatus};

use super::*;
use crate::store::tests::store_with_session;

/// A workspace being made, named and given an instance of its own, made `later` ms after `w`.
fn being_made(store: &Store, name: &str, later: i64) -> Workspace {
    let mut workspace = store.get_workspace(&"w".into()).unwrap();
    workspace.created_at_ms += later;
    workspace.id = name.into();
    workspace.name = name.into();
    workspace.instance_id = format!("{name}-instance").as_str().into();
    workspace.provider = "incus".into();
    workspace.status = WorkspaceStatus::Creating;
    store.insert_workspace(&workspace, None).unwrap();
    workspace
}

fn stages(store: &Store) -> Vec<(String, InstanceStage)> {
    store.recorded_instances().unwrap().into_iter().map(|i| (i.id.to_string(), i.stage)).collect()
}

#[test]
fn a_workspaces_instance_is_recorded_with_it_before_its_made_and_until_its_destroyed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let workspace = being_made(&store, "birch", 1);
    let recorded = store.recorded_instances().unwrap();
    store.instance_made(&workspace.instance_id).unwrap();
    let made = stages(&store);
    store.instance_destroying(&workspace.instance_id).unwrap();
    let destroying = stages(&store);
    store.instance_destroyed(&workspace.instance_id).unwrap();

    let birch = RecordedInstance {
        id: "birch-instance".into(),
        provider: "incus".into(),
        stage: InstanceStage::Creating,
        created_at_ms: workspace.created_at_ms,
    };
    assert_eq!(recorded[1], birch, "{recorded:?}");
    assert_eq!(recorded[0].stage, InstanceStage::Ready, "a workspace that's ready has one that's made");
    assert_eq!(made[1], ("birch-instance".into(), InstanceStage::Ready));
    assert_eq!(destroying[1], ("birch-instance".into(), InstanceStage::Destroying));
    assert_eq!(stages(&store), [("w".into(), InstanceStage::Ready)]);
}

/// As for a workspace being made whose instance is made again under another name.
#[test]
fn an_instance_destroyed_is_recorded_again_before_its_made_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let workspace = being_made(&store, "birch", 1);
    store.instance_made(&workspace.instance_id).unwrap();

    let while_recorded = store.instance_creating(&workspace.instance_id, &"incus".into());
    store.instance_destroying(&workspace.instance_id).unwrap();
    store.instance_destroyed(&workspace.instance_id).unwrap();
    store.instance_creating(&workspace.instance_id, &"incus".into()).unwrap();

    assert!(while_recorded.is_err(), "{while_recorded:?}");
    // By its ID, since it's recorded again at the time it is, which can be the millisecond `w` was made in, and then
    // it's listed first.
    let recorded = store.recorded_instances().unwrap();
    let again = recorded.iter().find(|instance| instance.id.as_str() == "birch-instance").unwrap();
    assert_eq!((again.provider.as_str(), again.stage), ("incus", InstanceStage::Creating), "{recorded:?}");
}

#[test]
fn a_workspace_given_another_workspaces_instance_isnt_made() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut workspace = store.get_workspace(&"w".into()).unwrap();
    workspace.id = "birch".into();
    workspace.name = "birch".into();

    let made = store.insert_workspace(&workspace, None);

    assert!(made.is_err(), "{made:?}");
    assert!(store.get_workspace(&"birch".into()).is_err(), "nor is the workspace recorded");
}

#[test]
fn a_destroy_doesnt_become_a_make_when_a_create_answers_late() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let workspace = being_made(&store, "birch", 1);

    store.instance_destroying(&workspace.instance_id).unwrap();
    store.instance_made(&workspace.instance_id).unwrap();

    assert_eq!(stages(&store)[1], ("birch-instance".into(), InstanceStage::Destroying));
}

/// So a crash between asking a provider for an instance and recording that it's made leaves one the next start
/// destroys, and so does a crash part way through destroying one.
#[test]
fn what_the_last_run_left_part_way_is_taken_to_be_destroyed_until_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    being_made(&store, "birch", 1);
    let destroying = being_made(&store, "cedar", 2);
    store.instance_made(&destroying.instance_id).unwrap();
    store.instance_destroying(&destroying.instance_id).unwrap();

    let unfinished = store.take_unfinished_instances().unwrap();
    let again = store.take_unfinished_instances().unwrap();

    let ids: Vec<_> = unfinished.iter().map(|i| (i.id.to_string(), i.stage)).collect();
    let expected = [("birch-instance", InstanceStage::Destroying), ("cedar-instance", InstanceStage::Destroying)];
    assert_eq!(ids, expected.map(|(id, stage)| (id.to_string(), stage)));
    assert_eq!(again, unfinished, "still there until they're destroyed");
    assert_eq!(stages(&store)[0], ("w".into(), InstanceStage::Ready), "one that's made is left alone");
}
