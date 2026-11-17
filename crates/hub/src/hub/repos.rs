//! Adding and listing repositories.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, RepoId};

use super::{Hub, new_id};
use crate::store::{Store, now_ms};

fn find_repo_root(path: &Path) -> Result<PathBuf, ApiError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| ApiError::new(ErrorCode::InvalidRequest, format!("{}: {e}", path.display())))?;
    canonical.ancestors().find(|dir| dir.join(".git").exists()).map(Path::to_path_buf).ok_or_else(|| {
        ApiError::new(ErrorCode::InvalidRequest, format!("{} isn't in a git repository", path.display()))
    })
}

impl Hub {
    /// Adds the repository whose checkout `path` is in, and has a worker on this machine read its address, as
    /// [`Hub::read_repo_urls_here`] says. Adding it again returns the first record, and reads its address again.
    pub(crate) async fn repo_add(self: &Arc<Self>, path: String) -> Result<Repo, ApiError> {
        let root = find_repo_root(Path::new(&path))?;
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, format!("{path} has no directory name")))?;
        let (id, checkout) = (RepoId::from(new_id()), root.to_string_lossy().into_owned());
        let repo = self.blocking(move |store| store.add_repo(&id, &name, &checkout, now_ms())).await?;
        self.notify(HubNotice::Repo(repo.clone()));
        self.read_repo_urls_here().await;
        Ok(repo)
    }

    pub(crate) async fn repo_list(&self) -> Result<Vec<Repo>, ApiError> {
        self.blocking(Store::list_repos).await
    }
}
