//! Recording instances, against a provider that does only what it's told.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use protocol::Build;
use protocol::model::{Workspace, WorkspaceStatus};
use providers::{Capabilities, Instance, ProviderError, Size, WorkerBinary, WorkerCommand};
use tokio::sync::Notify;

use super::*;
use crate::provider_set::ProviderSet;
use crate::store::tests::store_with_session;
use crate::store::{InstanceStage, Store};

/// Makes and destroys nothing, and says it has unless it's told to refuse. One that holds its first start says so on
/// `started`, and waits for `go_on` before refusing it.
#[derive(Default)]
struct Fake {
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

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    async fn create(
        &self,
        _: &InstanceId,
        _: Option<&InstanceId>,
        _: Option<Size>,
        _: &str,
    ) -> Result<(), ProviderError> {
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

    async fn stop(&self, _: &InstanceId) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn kill(&self, _: &InstanceId) -> Result<(), ProviderError> {
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
        Ok(Vec::new())
    }

    async fn connect(&self, _: &InstanceId) -> Result<WorkerCommand, ProviderError> {
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
    let hub = Hub::new(
        Build::UNSTAMPED,
        store,
        dir.join("attachments"),
        dir.join("worker-logs"),
        providers,
        Vec::new(),
        false,
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
        hub.blocking(move |store| store.insert_workspace(&workspace, None)).await.unwrap();
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
