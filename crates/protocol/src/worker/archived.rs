//! Capturing and restoring local branches and unpushed commits for ephemeral workers whose clones are destroyed on
//! archive.

use serde::{Deserialize, Serialize};

use super::repo_source::RepoSource;
use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{BranchName, CommitId};

/// Maximum raw git bundle size accepted when archiving an ephemeral workspace (128 MiB). Base64 adds one third (~171
/// MiB), leaving headroom under [`MAX_FRAME_BYTES`](crate::frame::MAX_FRAME_BYTES) (256 MiB) for JSON framing.
pub const MAX_ARCHIVED_BUNDLE_BYTES: usize = 128 * 1024 * 1024;

crate::base64_bytes::base64_bytes!(
    /// Raw git bundle bytes, serialized as base64 on the wire.
    ArchivedBundle
);

/// Captures local branches and unpushed commits from an ephemeral workspace's clone before its worktree is removed and
/// its instance is destroyed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CaptureArchive {
    pub source: RepoSource,
    /// When true, a bundle exceeding [`MAX_ARCHIVED_BUNDLE_BYTES`] is omitted (`bundle_omitted: true`) instead of
    /// refusing with `ErrorCode::NeedsForce`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
}

impl Request for CaptureArchive {
    type Reply = ArchivedGit;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::CaptureArchive(self)
    }
}

/// One local branch captured from an ephemeral workspace's clone before its instance is destroyed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ArchivedBranch {
    pub name: BranchName,
    pub commit: CommitId,
}

/// Local branches and unpushed commits captured when an ephemeral workspace is archived, carried between the worker
/// and hub across archive and restore.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ArchivedGit {
    /// Every local branch (`refs/heads/*`) present in the clone when archived, with its commit ID.
    pub branches: Vec<ArchivedBranch>,
    /// Git bundle of commits reachable from local branches and not from `origin`'s default branch
    /// (`refs/remotes/origin/HEAD`). `None` when every local branch commit is already reachable there, or when
    /// archiving was forced past [`MAX_ARCHIVED_BUNDLE_BYTES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<ArchivedBundle>,
    /// True when archiving was forced past [`MAX_ARCHIVED_BUNDLE_BYTES`] and the unpushed bundle was dropped, so
    /// restoring falls back to `origin/<branch>` or `origin/HEAD` if the branch's commit isn't in `origin`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bundle_omitted: bool,
}

impl Reply for ArchivedGit {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::ArchivedGit(archived) = response else { return Err(response) };
        Ok(archived)
    }
}
