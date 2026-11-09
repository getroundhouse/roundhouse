//! Adding and listing repositories.

use std::path::{Path, PathBuf};

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, Rev};

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
    pub(crate) async fn repo_add(&self, path: String) -> Result<Repo, ApiError> {
        let root = find_repo_root(Path::new(&path))?;
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, format!("{path} has no directory name")))?;
        let repo = Repo {
            id: new_id().into(),
            name,
            path: root.to_string_lossy().into_owned(),
            created_at_ms: now_ms(),
            rev: Rev::default(),
        };
        let repo = self.blocking(move |store| store.add_repo(&repo)).await?;
        self.notify(HubNotice::Repo(repo.clone()));
        Ok(repo)
    }

    pub(crate) async fn repo_list(&self) -> Result<Vec<Repo>, ApiError> {
        self.blocking(Store::list_repos).await
    }
}
