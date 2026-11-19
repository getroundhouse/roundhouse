//! Capturing and restoring local branches and unpushed commits for ephemeral workers whose clones are destroyed on
//! archive.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, CommitId};
use protocol::worker::{ArchivedBranch, ArchivedBundle, ArchivedGit, MAX_ARCHIVED_BUNDLE_BYTES};
use tracing::warn;

use super::super::{commit_of, git_error, git_ok, off_thread, ref_exists, reject_option_like, valid_branch_name};

const TEMP_DIR_NAME: &str = ".roundhouse-bundles";
const ORIGIN_HEAD: &str = "refs/remotes/origin/HEAD";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Removes a temporary bundle file when dropped so failures never leave bundles behind. Lives inside a private
/// `0700` subdirectory of the repository so `git bundle create` never exposes commits in a shared temp directory.
struct TempBundle(PathBuf);

impl TempBundle {
    async fn in_repo(repo: &Path) -> Result<Self, ApiError> {
        let dir = repo.join(TEMP_DIR_NAME);
        off_thread(dir.clone(), |path| private_files::create_dir_all(&path))
            .await
            .map_err(|e| git_error(format!("creating temporary bundle directory {}: {e}", dir.display())))?;
        let seq = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let name = format!("archive-{}-{seq}.bundle", std::process::id());
        Ok(Self(dir.join(name)))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempBundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Captures every local branch in `repo` and a git bundle of commits reachable from local branches and not from
/// `origin`'s default branch (`refs/remotes/origin/HEAD`). Refuses with `NeedsForce` when the raw bundle exceeds
/// [`MAX_ARCHIVED_BUNDLE_BYTES`] unless `force` is set, in which case the oversized bundle is dropped and
/// `bundle_omitted` is set.
pub(crate) async fn capture_archived(given: &Environment, repo: &Path, force: bool) -> Result<ArchivedGit, ApiError> {
    capture_archived_with_limit(given, repo, force, MAX_ARCHIVED_BUNDLE_BYTES).await
}

pub(super) async fn capture_archived_with_limit(
    given: &Environment,
    repo: &Path,
    force: bool,
    max_bundle_bytes: usize,
) -> Result<ArchivedGit, ApiError> {
    let branches = list_local_branches(given, repo).await?;
    if branches.is_empty() {
        return Ok(ArchivedGit { branches, bundle: None, bundle_omitted: false });
    }
    let has_origin_head = ref_exists(given, repo, ORIGIN_HEAD).await?;
    let rev_args: &[&str] = if has_origin_head {
        &["rev-list", "--count", "--branches", "--not", ORIGIN_HEAD]
    } else {
        &["rev-list", "--count", "--branches"]
    };
    let count = git_ok(given, repo, rev_args).await?;
    let unpushed: usize =
        count.trim().parse().map_err(|e| git_error(format!("unexpected git rev-list --count output: {e}")))?;
    if unpushed == 0 {
        return Ok(ArchivedGit { branches, bundle: None, bundle_omitted: false });
    }
    let (bundle, bundle_omitted) = create_bundle(given, repo, has_origin_head, force, max_bundle_bytes).await?;
    Ok(ArchivedGit { branches, bundle, bundle_omitted })
}

async fn list_local_branches(given: &Environment, repo: &Path) -> Result<Vec<ArchivedBranch>, ApiError> {
    // `lstrip=2` strips `refs/heads/` reliably even when a tag shares the branch's name (where `:short` adds `heads/`).
    let output =
        git_ok(given, repo, &["for-each-ref", "--format=%(refname:lstrip=2)%00%(objectname)", "refs/heads"]).await?;
    let mut branches = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((name, commit)) = line.split_once('\0') else {
            return Err(git_error(format!("unexpected git for-each-ref line: {line}")));
        };
        branches.push(ArchivedBranch { name: BranchName::from(name), commit: CommitId::from(commit) });
    }
    Ok(branches)
}

async fn create_bundle(
    given: &Environment,
    repo: &Path,
    has_origin_head: bool,
    force: bool,
    max_bundle_bytes: usize,
) -> Result<(Option<ArchivedBundle>, bool), ApiError> {
    let temp = TempBundle::in_repo(repo).await?;
    let target = temp.path().to_string_lossy().into_owned();
    let args: Vec<&str> = if has_origin_head {
        vec!["bundle", "create", "--quiet", &target, "--branches", "--not", ORIGIN_HEAD]
    } else {
        vec!["bundle", "create", "--quiet", &target, "--branches"]
    };
    git_ok(given, repo, &args).await?;
    let size = off_thread(temp.path().to_path_buf(), |path| Ok(std::fs::metadata(&path)?.len()))
        .await
        .map_err(|e| git_error(format!("reading archive bundle size: {e}")))?;
    if size > max_bundle_bytes as u64 {
        if !force {
            let actual_mib = size.div_ceil(1024 * 1024);
            let limit_mib = (max_bundle_bytes as u64).div_ceil(1024 * 1024);
            return Err(ApiError::new(
                ErrorCode::NeedsForce,
                format!(
                    "archived commits are {actual_mib} MiB, over the {limit_mib} MiB limit; push them first or force \
                     archive without keeping them"
                ),
            ));
        }
        warn!(bytes = size, limit = max_bundle_bytes, "dropping oversized archive bundle on force archive");
        return Ok((None, true));
    }
    let bytes = off_thread(temp.path().to_path_buf(), |path| std::fs::read(&path))
        .await
        .map_err(|e| git_error(format!("reading archive bundle: {e}")))?;
    Ok((Some(ArchivedBundle::new(bytes)), false))
}

/// Imports `archived`'s bundle (if any) into `repo` and creates each recorded local branch ref that doesn't already
/// exist. Existing branches are never overwritten, so a surviving or shared clone never rewinds live work. When
/// `archived.bundle_omitted` is true and `target_branch` isn't present after unbundling, creates it from
/// `refs/remotes/origin/<target_branch>` or `refs/remotes/origin/HEAD` so a force-archived oversized workspace can
/// still be restored.
pub(crate) async fn restore_archived(
    given: &Environment,
    repo: &Path,
    archived: &ArchivedGit,
    target_branch: &BranchName,
) -> Result<(), ApiError> {
    if let Some(bundle) = &archived.bundle {
        let bytes = bundle.as_slice().to_vec();
        let temp = TempBundle::in_repo(repo).await?;
        off_thread(temp.path().to_path_buf(), move |path| {
            let mut file = private_files::file_options().create_new(true).write(true).open(&path)?;
            file.write_all(&bytes)?;
            file.flush()
        })
        .await
        .map_err(|e| git_error(format!("writing archived bundle to unbundle: {e}")))?;
        let source = temp.path().to_string_lossy().into_owned();
        git_ok(given, repo, &["bundle", "unbundle", &source]).await?;
    }
    for branch in &archived.branches {
        restore_branch_if_absent(given, repo, branch).await?;
    }
    if archived.bundle_omitted {
        ensure_target_branch_after_omitted_bundle(given, repo, target_branch).await?;
    }
    Ok(())
}

async fn restore_branch_if_absent(given: &Environment, repo: &Path, branch: &ArchivedBranch) -> Result<(), ApiError> {
    valid_branch_name(given, repo, &branch.name).await?;
    reject_option_like(branch.commit.as_str(), "a commit ID")?;
    let ref_name = format!("refs/heads/{}", branch.name);
    let Some(resolved) = commit_of(given, repo, branch.commit.as_str()).await? else {
        warn!(branch = %branch.name, commit = %branch.commit, "skipping archived branch whose commit isn't in the clone");
        return Ok(());
    };
    if let Some(existing) = commit_of(given, repo, &ref_name).await? {
        if existing != resolved {
            warn!(
                branch = %branch.name,
                existing = %existing,
                archived = %resolved,
                "leaving existing branch untouched during archive restore"
            );
        }
        return Ok(());
    }
    // Empty old-value creates the ref atomically only if absent, never overwriting a branch created concurrently.
    git_ok(given, repo, &["update-ref", &ref_name, resolved.as_str(), ""]).await?;
    Ok(())
}

async fn ensure_target_branch_after_omitted_bundle(
    given: &Environment,
    repo: &Path,
    target_branch: &BranchName,
) -> Result<(), ApiError> {
    valid_branch_name(given, repo, target_branch).await?;
    let ref_name = format!("refs/heads/{target_branch}");
    if ref_exists(given, repo, &ref_name).await? {
        return Ok(());
    }
    let remote_branch = format!("refs/remotes/origin/{target_branch}");
    let fallback = match commit_of(given, repo, &remote_branch).await? {
        Some(commit) => Some((remote_branch, commit)),
        None => commit_of(given, repo, ORIGIN_HEAD).await?.map(|commit| (ORIGIN_HEAD.to_string(), commit)),
    };
    if let Some((from, commit)) = fallback {
        warn!(
            branch = %target_branch,
            %from,
            %commit,
            "restoring force-archived workspace whose oversized bundle was omitted from fallback ref"
        );
        git_ok(given, repo, &["update-ref", &ref_name, commit.as_str(), ""]).await?;
    }
    Ok(())
}
