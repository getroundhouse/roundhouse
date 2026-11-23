//! Recording instances, against a provider that does only what it's told.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use protocol::Build;
use protocol::client::{ApiError, HubStart};
use protocol::model::{Workspace, WorkspaceId, WorkspaceStatus};
use providers::{Capabilities, Instance, InstanceState, ProviderError, Size, WorkerBinary, WorkerCommand};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::*;
use crate::hub::HubDirs;
use crate::hub::tests::NO_GITHUB;
use crate::provider_set::ProviderSet;
use crate::store::tests::store_with_session;
use crate::store::{InstanceStage, Store};
use crate::timers::Timers;
use crate::worker_link::WorkerLink;
use github::Github;

/// Makes and destroys nothing, and says it has unless it's told to refuse. One that holds its first start says so on
/// `started`, and waits for `go_on` before refusing it. It lists what `listed` says, and keeps which instances it was
/// asked how to reach a worker on, which it refuses.
#[derive(Default)]
struct Fake {
    listed: Vec<Instance>,
    connected: parking_lot::Mutex<Vec<InstanceId>>,
    refuses_to_create: bool,
    refuses_to_destroy: AtomicBool,
    holds_start: AtomicBool,
    started: Notify,
    go_on: Notify,
    destroyed: parking_lot::Mutex<Vec<InstanceId>>,
}

fn refused(what: &str) -> ProviderError {
    ProviderError::io(what, std::io::Error::other("refused"))
}

#[async_trait]
impl Provider for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn starts_or_stops_within(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    async fn environment(&self) -> Result<providers::WorkerEnvironment, ProviderError> {
        Ok(providers::WorkerEnvironment::HUB_MACHINE)
    }

    async fn create(&self, _: &InstanceId, _: Option<&InstanceId>, _: Option<Size>) -> Result<(), ProviderError> {
        if self.refuses_to_create { Err(refused("creating")) } else { Ok(()) }
    }

    async fn start(&self, _: &InstanceId, _: &[WorkerBinary]) -> Result<(), ProviderError> {
        if !self.holds_start.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        self.started.notify_one();
        self.go_on.notified().await;
        Err(refused("starting"))
    }

    async fn end_worker(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn kill_worker(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        if self.refuses_to_destroy.load(Ordering::Relaxed) {
            return Err(refused("destroying"));
        }
        self.destroyed.lock().push(instance_id.clone());
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        Ok(self.listed.clone())
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        self.connected.lock().push(instance_id.clone());
        Err(ProviderError::Unsupported("run a worker"))
    }

    async fn make_base_image(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make base images"))
    }
}

/// A hub whose store has the workspace `w`, and whose only provider is `fake`.
fn hub_with(dir: &Path, fake: Fake) -> (Arc<Hub>, Arc<Fake>) {
    let fake = Arc::new(fake);
    let providers = ProviderSet::only(fake.clone());
    let store = Arc::new(store_with_session(dir));
    let github = Github::new(github::GithubConfig::at(NO_GITHUB.to_string()));
    let hub = Hub::new(
        Build::UNSTAMPED,
        store,
        HubDirs::in_dir(dir),
        providers,
        github,
        crate::machine_environment::MachineEnvironment::only(&[]),
        Vec::new(),
        HubStart::ByHand,
        Timers::DEFAULT,
    );
    (Arc::new(hub), fake)
}

async fn stages(hub: &Hub) -> Vec<(String, InstanceStage)> {
    let recorded = hub.blocking(Store::recorded_instances).await.unwrap();
    recorded.into_iter().map(|instance| (instance.id.to_string(), instance.stage)).collect()
}

#[tokio::test]
async fn an_instance_its_provider_couldnt_make_is_destroyed_and_its_record_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, fake) = hub_with(dir.path(), Fake { refuses_to_create: true, ..Fake::default() });

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await;

    assert!(made.is_err(), "{made:?}");
    assert_eq!(fake.destroyed.lock().len(), 1, "in case it made part of one");
    assert_eq!(stages(&hub).await, [("w".to_string(), InstanceStage::Ready)]);
}

/// A crash between asking the provider for an instance and recording that it's made leaves it being made. The next
/// start destroys it, and keeps trying, start after start, until its provider has.
#[tokio::test]
async fn an_instance_the_last_run_left_part_way_is_destroyed_once_its_provider_can() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake { refuses_to_destroy: AtomicBool::new(true), ..Fake::default() };
    let (hub, fake) = hub_with(dir.path(), fake);
    let mut half_made: Workspace = hub.workspace(&"w".into()).await.unwrap();
    (half_made.id, half_made.name, half_made.instance_id) = ("birch".into(), "birch".into(), "birch-vm".into());
    (half_made.provider, half_made.status) = ("fake".into(), WorkspaceStatus::Creating);
    half_made.created_at_ms += 1;
    let mut elsewhere = half_made.clone();
    (elsewhere.id, elsewhere.name, elsewhere.instance_id) = ("cedar".into(), "cedar".into(), "cedar-vm".into());
    elsewhere.provider = "gone".into();
    elsewhere.created_at_ms += 1;
    for workspace in [half_made, elsewhere] {
        hub.blocking(move |store| {
            store.insert_workspace(&workspace, &crate::store::WorkspacePlacement::local_checkout("/repo"), None)
        })
        .await
        .unwrap();
    }
    // As the next start finds it, then the one after.
    let next_start = || async {
        hub.blocking(|store| {
            store.take_unfinished_workspaces()?;
            store.take_unfinished_instances()
        })
        .await
        .unwrap()
    };

    hub.clone().settle_providers(next_start().await).await;
    let after_a_refusal = stages(&hub).await;
    fake.refuses_to_destroy.store(false, Ordering::Relaxed);
    hub.clone().settle_providers(next_start().await).await;

    let destroying = |id: &str| (id.to_string(), InstanceStage::Destroying);
    let ready = ("w".to_string(), InstanceStage::Ready);
    assert_eq!(after_a_refusal, [ready.clone(), destroying("birch-vm"), destroying("cedar-vm")]);
    assert_eq!(*fake.destroyed.lock(), [InstanceId::from("birch-vm")]);
    assert_eq!(stages(&hub).await, [ready, destroying("cedar-vm")], "one whose provider isn't set up is left");
}

/// Marked made once its provider has made it, and to be destroyed before its provider is asked to destroy it, so one
/// its provider can't destroy is destroyed at the next start, however far it got.
#[tokio::test]
async fn an_instance_is_marked_made_once_its_made_and_to_be_destroyed_before_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let fake =
        Fake { holds_start: AtomicBool::new(true), refuses_to_destroy: AtomicBool::new(true), ..Fake::default() };
    let (hub, fake) = hub_with(dir.path(), fake);
    let birch =
        |stages: Vec<(String, InstanceStage)>| stages.into_iter().find(|(id, _)| id != "w").map(|(_, stage)| stage);

    let making = tokio::spawn({
        let hub = hub.clone();
        async move { hub.workspace_create("r".into(), Some("birch".into()), None, None).await }
    });
    fake.started.notified().await;
    let as_its_worker_starts = birch(stages(&hub).await);
    fake.go_on.notify_one();
    let made = making.await.unwrap();

    assert!(made.is_err(), "its worker never started: {made:?}");
    assert_eq!(as_its_worker_starts, Some(InstanceStage::Ready));
    assert_eq!(birch(stages(&hub).await), Some(InstanceStage::Destroying), "its provider couldn't destroy it");
}

/// Adds the workspace `birch`, on `fake`, whose gate says nothing but its own worker can be on its instance, as once
/// one has started, and starts its worker, on a task of its own.
async fn starting_birch(hub: &Arc<Hub>) -> (WorkspaceId, JoinHandle<Result<Arc<WorkerLink>, ApiError>>) {
    let mut birch: Workspace = hub.workspace(&"w".into()).await.unwrap();
    (birch.id, birch.name, birch.instance_id, birch.provider) =
        ("birch".into(), "birch".into(), "birch-vm".into(), "fake".into());
    birch.created_at_ms += 1;
    hub.blocking(move |store| {
        store.insert_workspace(&birch, &crate::store::WorkspacePlacement::local_checkout("/repo"), None)
    })
    .await
    .unwrap();
    let id = WorkspaceId::from("birch");
    *hub.worker_gate(&id).lock().await = true;
    let starting = tokio::spawn({
        let (hub, id) = (hub.clone(), id.clone());
        async move { hub.ensure_worker(&id).await }
    });
    (id, starting)
}

/// A start cut short as its worker starts leaves the next to clear the instance first, since its worker may be there.
#[tokio::test]
async fn a_start_dropped_partway_leaves_the_next_to_clear_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, fake) = hub_with(dir.path(), Fake { holds_start: AtomicBool::new(true), ..Fake::default() });
    let (id, starting) = starting_birch(&hub).await;

    fake.started.notified().await;
    starting.abort();
    let _ = starting.await;

    assert!(!*hub.worker_gate(&id).lock().await, "its worker may be on the instance");
}

/// A start that fails may not have ended what it started, so it leaves the next to clear the instance first.
#[tokio::test]
async fn a_start_that_fails_leaves_the_next_to_clear_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, fake) = hub_with(dir.path(), Fake { holds_start: AtomicBool::new(true), ..Fake::default() });
    let (id, starting) = starting_birch(&hub).await;

    fake.started.notified().await;
    fake.go_on.notify_one();
    let started = starting.await.unwrap();

    assert!(started.is_err(), "its provider refused to start it");
    assert!(!*hub.worker_gate(&id).lock().await, "what it started may be on the instance");
}

/// A starting hub carries on only where its provider says a worker serves the instance, and leaves one that's awake
/// with nothing on it, which every instance of a provider that keeps them awake is once its worker has ended.
#[tokio::test]
async fn a_starting_hub_carries_on_only_where_a_worker_serves_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let listed = |id: &str, worker_running| Instance {
        instance_id: id.into(),
        state: InstanceState::Running,
        base_image: false,
        worker_running,
    };
    let fake = Fake { listed: vec![listed("birch-vm", true), listed("cedar-vm", false)], ..Fake::default() };
    let (hub, fake) = hub_with(dir.path(), fake);
    let mut birch: Workspace = hub.workspace(&"w".into()).await.unwrap();
    for (id, instance_id) in [("birch", "birch-vm"), ("cedar", "cedar-vm")] {
        (birch.id, birch.name, birch.instance_id, birch.provider) =
            (id.into(), id.into(), instance_id.into(), "fake".into());
        birch.created_at_ms += 1;
        let workspace = birch.clone();
        let placement = crate::store::WorkspacePlacement::local_checkout("/repo");
        hub.blocking(move |store| store.insert_workspace(&workspace, &placement, None)).await.unwrap();
    }

    hub.carry_on(crate::hub::LeftAtWork::read(&hub.store).unwrap()).await;

    assert_eq!(*fake.connected.lock(), [InstanceId::from("birch-vm")]);
}
