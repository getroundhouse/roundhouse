//! Archive, restore, and crash-settling steps specific to workspaces whose repository is a clone on
//! `ClonePersistence::Ephemeral` compute, whose instances and clones are destroyed on archive and recreated on
//! restore.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{InstanceId, Workspace};
use protocol::provider::ClonePersistence;
use protocol::worker::{CaptureArchive, RepoSource, RestoreWorkspace, WorktreeThere, WorktreeThereReply};
use providers::Provider;
use tracing::warn;

use crate::error::internal;
use crate::hub::Hub;
use crate::store::{InstanceStage, Move, RecordedInstance, WorkspacePlacement};
use crate::worker_link::WorkerLink;

/// Whether archiving this workspace destroys its repository clone and must capture and restore via a hub bundle. A
/// workspace on a hub checkout never destroys the checkout, even if its provider declares `Ephemeral`.
pub(super) fn uses_ephemeral_clone(placement: &WorkspacePlacement) -> bool {
    placement.environment.clones == ClonePersistence::Ephemeral && matches!(placement.source, RepoSource::Clone { .. })
}

impl Hub {
    async fn recorded_instance(&self, id: &InstanceId) -> Result<Option<RecordedInstance>, ApiError> {
        let owned = id.clone();
        self.blocking(move |store| store.recorded_instance(&owned)).await
    }

    /// Probes whether capturing the archive bundle would trip the size cap, returning the `NeedsForce` message to
    /// combine with dirty/unmerged losses before anything changes. Non-`NeedsForce` errors are ignored here so the
    /// normal archive flow surfaces them at the right stage.
    pub(super) async fn probe_oversized_archive(link: &WorkerLink, source: &RepoSource) -> Option<String> {
        match link.call(CaptureArchive { source: source.clone(), force: false }).await {
            Err(e) if e.code == ErrorCode::NeedsForce => Some(e.message),
            _ => None,
        }
    }

    /// Captures local branches and unpushed commits from the worker and writes them atomically to `bundles/` before
    /// the worktree is removed.
    pub(super) async fn capture_and_save_archive(
        &self,
        workspace: &Workspace,
        link: &WorkerLink,
        source: &RepoSource,
        force: bool,
    ) -> Result<(), ApiError> {
        let archived = link.call(CaptureArchive { source: source.clone(), force }).await?;
        self.save_archived_git(&workspace.id, &archived).await
    }

    /// Closes the worker and destroys an ephemeral workspace's instance once its bundle is on disk and its worktree is
    /// gone. A transient `provider.destroy` failure is logged rather than returned because the bundle is already synced
    /// and the instance is marked `Destroying` in SQLite for startup or restore to finish.
    pub(super) async fn destroy_ephemeral_instance(
        &self,
        workspace: &Workspace,
        link: &WorkerLink,
        provider: &dyn Provider,
    ) {
        self.close_worker(workspace, link, provider).await;
        self.destroy_instance_best_effort(provider, &workspace.instance_id).await;
    }

    /// Restores an archived ephemeral workspace. If no hub bundle exists and the recorded instance is still `Ready`
    /// (archived by an older build before bundles existed), restores in place without destroying the only copy of its
    /// commits. Otherwise destroys any leftover instance, creates a fresh instance, and restores from the saved bundle.
    pub(super) async fn restore_ephemeral(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        placement: WorkspacePlacement,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        let archived = self.load_archived_git(&workspace.id).await?;
        let recorded = self.recorded_instance(&workspace.instance_id).await?;
        if archived.is_none() && recorded.as_ref().is_some_and(|r| r.stage == InstanceStage::Ready) {
            let link = self.ensure_worker(&workspace.id).await?;
            let request =
                RestoreWorkspace { source: placement.source, path, branch: workspace.branch.clone(), archived: None };
            link.call(request).await?;
            return Ok(link);
        }
        let provider = self.provider(&workspace.provider)?;
        if recorded.is_some() {
            self.destroy_instance(provider.as_ref(), &workspace.instance_id).await?;
        }
        let (instance_id, provider_name) = (workspace.instance_id.clone(), workspace.provider.clone());
        self.blocking(move |store| store.instance_creating(&instance_id, &provider_name)).await?;
        if let Err(e) = self.make_workspace_instance(workspace, provider.as_ref()).await {
            self.destroy_instance_best_effort(provider.as_ref(), &workspace.instance_id).await;
            return Err(e);
        }
        let request = RestoreWorkspace { source: placement.source, path, branch: workspace.branch.clone(), archived };
        let restored = async {
            let link = self.ensure_worker(&workspace.id).await?;
            link.call(request).await?;
            Ok::<_, ApiError>(link)
        }
        .await;
        match restored {
            Ok(link) => Ok(link),
            Err(e) => {
                if let Some(link) = self.existing_worker(&workspace.id) {
                    self.close_worker(workspace, &link, provider.as_ref()).await;
                }
                self.destroy_instance_best_effort(provider.as_ref(), &workspace.instance_id).await;
                Err(e)
            }
        }
    }

    /// Settles an interrupted archive or restore on an ephemeral workspace, finishing any unfinished `Creating` or
    /// `Destroying` instance for this workspace directly so local moves never wait on remote adapters.
    pub(super) async fn settle_ephemeral_move(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        placement: WorkspacePlacement,
        moving: Move,
    ) -> Result<Workspace, ApiError> {
        let recorded = self.recorded_instance(&workspace.instance_id).await?;
        match moving {
            Move::Archive => self.settle_ephemeral_archive(workspace, path, placement, recorded).await,
            Move::Restore => self.settle_ephemeral_restore(workspace, path, recorded).await,
        }
    }

    async fn settle_ephemeral_archive(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        placement: WorkspacePlacement,
        recorded: Option<RecordedInstance>,
    ) -> Result<Workspace, ApiError> {
        let id = &workspace.id;
        let provider = self.provider(&workspace.provider)?;
        if recorded.as_ref().is_some_and(|r| r.stage == InstanceStage::Ready) {
            let link = self.ensure_worker(id).await?;
            let WorktreeThereReply { there } = link.call(WorktreeThere { path }).await?;
            if there {
                self.remove_archived_git(id).await;
                return self.end_move_put_back(id, Move::Archive).await;
            }
            if !self.has_archived_git(id).await {
                // Never force during crash recovery: an oversized bundle nobody chose to drop must not be discarded.
                self.capture_and_save_archive(workspace, &link, &placement.source, false).await?;
            }
            self.destroy_ephemeral_instance(workspace, &link, provider.as_ref()).await;
            return self.record_archived(id, super::archiving::checked_out_branch(workspace)).await;
        }
        if recorded.is_some() {
            if let Some(link) = self.existing_worker(id) {
                self.close_worker(workspace, &link, provider.as_ref()).await;
            }
            self.destroy_instance_best_effort(provider.as_ref(), &workspace.instance_id).await;
        }
        if self.has_archived_git(id).await {
            return self.record_archived(id, super::archiving::checked_out_branch(workspace)).await;
        }
        Err(internal("an ephemeral workspace being archived lost its instance without a saved bundle"))
    }

    async fn settle_ephemeral_restore(
        self: &Arc<Self>,
        workspace: &Workspace,
        path: String,
        recorded: Option<RecordedInstance>,
    ) -> Result<Workspace, ApiError> {
        let id = &workspace.id;
        match recorded {
            None => self.end_move_put_back(id, Move::Restore).await,
            Some(r) if r.stage != InstanceStage::Ready => {
                let provider = self.provider(&workspace.provider)?;
                self.destroy_instance_best_effort(provider.as_ref(), &workspace.instance_id).await;
                self.end_move_put_back(id, Move::Restore).await
            }
            Some(_) => {
                let link = self.ensure_worker(id).await?;
                let WorktreeThereReply { there } = link.call(WorktreeThere { path }).await?;
                if there {
                    return self.record_restored(id, &link).await;
                }
                let provider = self.provider(&workspace.provider)?;
                self.destroy_ephemeral_instance(workspace, &link, provider.as_ref()).await;
                self.end_move_put_back(id, Move::Restore).await
            }
        }
    }

    async fn destroy_instance_best_effort(&self, provider: &dyn Provider, id: &InstanceId) {
        if let Err(e) = self.destroy_instance(provider, id).await {
            warn!(instance_id = %id, "destroying ephemeral instance failed; will retry on restart or restore: {}", e.message);
        }
    }
}
