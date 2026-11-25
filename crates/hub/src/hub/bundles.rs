//! On-disk storage of archived git state in `<data_dir>/bundles/` for workspaces whose provider destroys repository
//! clones on archive. Each archive writes raw git bundle bytes to `<workspace_id>.bundle` (when present) and then
//! hub-owned metadata to `<workspace_id>.json` as the atomic commit marker, decoupled from the wire type.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use protocol::client::ApiError;
use protocol::model::{BranchName, CommitId, WorkspaceId, WorkspaceStatus};
use protocol::paths::file_name_id;
use protocol::worker::{ArchivedBranch, ArchivedBundle, ArchivedGit};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::{Hub, new_id, off_thread};
use crate::error::{JsonProblem, internal};
use crate::lasting;

#[derive(Serialize, Deserialize)]
struct StoredBranch {
    name: BranchName,
    commit: CommitId,
}

#[derive(Serialize, Deserialize)]
struct StoredArchive {
    branches: Vec<StoredBranch>,
    #[serde(default)]
    has_bundle: bool,
    #[serde(default)]
    bundle_omitted: bool,
}

impl Hub {
    fn archive_paths(&self, id: &WorkspaceId) -> Result<(PathBuf, PathBuf), ApiError> {
        let safe = file_name_id("a workspace's", id.as_str())?;
        Ok((self.bundles_dir.join(format!("{safe}.json")), self.bundles_dir.join(format!("{safe}.bundle"))))
    }

    /// Writes `archived` atomically: raw bundle bytes to `<id>.bundle` first (if present), then metadata to `<id>.json`
    /// as the commit marker, using `lasting` directory creation and fsyncs.
    pub(super) async fn save_archived_git(&self, id: &WorkspaceId, archived: &ArchivedGit) -> Result<(), ApiError> {
        let (meta_path, bundle_path) = self.archive_paths(id)?;
        let dir = self.bundles_dir.clone();
        let stored = StoredArchive {
            branches: archived
                .branches
                .iter()
                .map(|b| StoredBranch { name: b.name.clone(), commit: b.commit.clone() })
                .collect(),
            has_bundle: archived.bundle.is_some(),
            bundle_omitted: archived.bundle_omitted,
        };
        let meta_bytes = serde_json::to_vec(&stored)
            .map_err(|e| internal(format!("encoding archived git metadata for {id}: {e}")))?;
        let bundle_bytes = archived.bundle.as_ref().map(|b| b.as_slice().to_vec());
        let (tmp_bundle, tmp_meta) = (
            dir.join(format!(".{}.{}.bundle.tmp", id.as_str(), new_id())),
            dir.join(format!(".{}.{}.json.tmp", id.as_str(), new_id())),
        );
        let owned = id.clone();
        off_thread(move || {
            lasting::create_dir_all(&dir)
                .map_err(|e| internal(format!("creating archive bundle directory for {owned}: {e}")))?;
            match bundle_bytes {
                Some(bytes) => write_atomic(&dir, &tmp_bundle, &bundle_path, &bytes)
                    .map_err(|e| internal(format!("saving archive bundle for {owned}: {e}")))?,
                None => remove_if_present(&bundle_path)
                    .map_err(|e| internal(format!("clearing stale archive bundle for {owned}: {e}")))?,
            }
            write_atomic(&dir, &tmp_meta, &meta_path, &meta_bytes)
                .map_err(|e| internal(format!("saving archive metadata for {owned}: {e}")))
        })
        .await
    }

    /// Reads the saved [`ArchivedGit`] for `id`, or `None` if no committed metadata file exists.
    pub(super) async fn load_archived_git(&self, id: &WorkspaceId) -> Result<Option<ArchivedGit>, ApiError> {
        let (meta_path, bundle_path) = self.archive_paths(id)?;
        let owned = id.clone();
        off_thread(move || load_stored(&owned, &meta_path, &bundle_path)).await
    }

    /// Whether a committed archive metadata file exists on disk for `id`.
    pub(super) async fn has_archived_git(&self, id: &WorkspaceId) -> bool {
        let Ok((meta_path, _)) = self.archive_paths(id) else { return false };
        off_thread(move || Ok::<_, ApiError>(meta_path.exists())).await.unwrap_or(false)
    }

    /// Removes the metadata commit marker first and then any raw bundle file for `id`.
    pub(super) async fn remove_archived_git(&self, id: &WorkspaceId) {
        let Ok((meta_path, bundle_path)) = self.archive_paths(id) else { return };
        let owned = id.clone();
        let removed = off_thread(move || {
            remove_if_present(&meta_path)
                .map_err(|e| internal(format!("removing archive metadata for {owned}: {e}")))?;
            remove_if_present(&bundle_path).map_err(|e| internal(format!("removing archive bundle for {owned}: {e}")))
        })
        .await;
        if let Err(e) = removed {
            warn!(
                name: "bundle.unremoved",
                workspace_id = %id,
                error = e.message,
                "Couldn't remove an archived workspace's bundle"
            );
        }
    }

    /// Removes temporary files and archive files left behind for workspaces that are already `Ready` or no longer
    /// exist. Files modified at or after `started_at` are skipped so concurrent archives can never be raced.
    pub(crate) async fn tidy_bundles(&self) {
        self.tidy_bundles_at(SystemTime::now()).await;
    }

    pub(super) async fn tidy_bundles_at(&self, started_at: SystemTime) {
        let keep: HashSet<String> = match self.blocking(|store| store.list_workspaces(true)).await {
            Ok(workspaces) => workspaces
                .into_iter()
                .filter(|w| {
                    matches!(
                        w.status,
                        WorkspaceStatus::Archived | WorkspaceStatus::Archiving | WorkspaceStatus::Restoring
                    )
                })
                .map(|w| w.id.to_string())
                .collect(),
            Err(e) => {
                return warn!(
                    name: "bundle.tidy_skipped",
                    error = e.message,
                    "Skipping a tidy of the archive bundles"
                );
            }
        };
        let dir = self.bundles_dir.clone();
        let result = off_thread(move || {
            remove_stale_bundles(&dir, &keep, started_at)
                .map_err(|e| internal(format!("tidying archive bundles failed: {e}")))
        })
        .await;
        if let Err(e) = result {
            warn!(name: "bundle.untidied", error = e.message, "Couldn't tidy the archive bundles");
        }
    }
}

fn load_stored(id: &WorkspaceId, meta_path: &Path, bundle_path: &Path) -> Result<Option<ArchivedGit>, ApiError> {
    let raw_meta = match std::fs::read(meta_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(internal(format!("reading archive metadata for {id}: {e}"))),
    };
    let stored: StoredArchive = serde_json::from_slice(&raw_meta)
        .map_err(|e| internal(format!("decoding archive metadata for {id}: {}", JsonProblem::from(e))))?;
    let bundle = if stored.has_bundle {
        let bytes =
            std::fs::read(bundle_path).map_err(|e| internal(format!("reading archive bundle for {id}: {e}")))?;
        Some(ArchivedBundle::new(bytes))
    } else {
        None
    };
    Ok(Some(ArchivedGit {
        branches: stored.branches.into_iter().map(|b| ArchivedBranch { name: b.name, commit: b.commit }).collect(),
        bundle,
        bundle_omitted: stored.bundle_omitted,
    }))
}

fn write_atomic(dir: &Path, tmp: &Path, dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let write_result = (|| {
        let mut file = private_files::file_options().create_new(true).write(true).open(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(tmp, dest) {
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    lasting::sync_dir(dir)
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn remove_stale_bundles(dir: &Path, keep: &HashSet<String>, started_at: SystemTime) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let modified = entry.metadata().and_then(|meta| meta.modified());
        if !modified.is_ok_and(|mtime| mtime < started_at) {
            continue;
        }
        let name = entry.file_name();
        let Some(text) = name.to_str() else { continue };
        let keep_file =
            text.strip_suffix(".json").or_else(|| text.strip_suffix(".bundle")).is_some_and(|id| keep.contains(id));
        if !keep_file && let Err(e) = std::fs::remove_file(entry.path()) {
            warn!(
                name: "bundle.stale_unremoved",
                path = %entry.path().display(),
                error = %e,
                "Couldn't remove a stale archive file"
            );
        }
    }
    Ok(())
}
