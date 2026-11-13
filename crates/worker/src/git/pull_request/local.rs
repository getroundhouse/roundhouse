//! How a worktree stands against its pull request: what it has that GitHub doesn't, and the other way.

use std::path::Path;

use protocol::model::Local;

use crate::git::{git, ref_exists, status};

/// Against the pull request's head where the worktree has that commit, which it does once it's pushed from here or
/// fetched. Otherwise against the branch's upstream, as of the last fetch.
pub(super) async fn local(path: &str, head: Option<&str>) -> Local {
    let Ok(status) = status(path).await else { return Local::default() };
    let (worktree, upstream) = (Path::new(path), (status.ahead, status.behind));
    let (ahead, behind) = match head {
        // A head that isn't here is at least one commit that isn't, such as one GitHub's Update branch or someone else
        // pushed. How many can't be told without fetching, so the upstream says, if it's been fetched since.
        Some(head) if !ref_exists(worktree, head).await.unwrap_or(true) => (upstream.0, upstream.1.max(1)),
        Some(head) => match apart(worktree, head).await.unwrap_or(upstream) {
            (0, behind) => (0, behind),
            (ahead, behind) => (not_pushed(worktree, head).await.unwrap_or(ahead), behind),
        },
        None => upstream,
    };
    Local { uncommitted: u32::try_from(status.files.len()).unwrap_or(u32::MAX), ahead, behind }
}

/// How many commits `HEAD` has that neither `head` nor any remote branch does. A push moves the remote branch here at
/// once, and GitHub's pull request a moment later.
async fn not_pushed(worktree: &Path, head: &str) -> Option<u32> {
    let output = git(worktree, &["rev-list", "--count", "HEAD", "--not", head, "--remotes"]).await.ok()?;
    if !output.success {
        return None;
    }
    output.stdout.trim().parse().ok()
}

/// How many commits `HEAD` has that `head` doesn't, and the other way.
async fn apart(worktree: &Path, head: &str) -> Option<(u32, u32)> {
    let range = format!("HEAD...{head}");
    let output = git(worktree, &["rev-list", "--left-right", "--count", &range]).await.ok()?;
    if !output.success {
        return None;
    }
    let mut counts = output.stdout.split_whitespace().map(str::parse);
    Some((counts.next()?.ok()?, counts.next()?.ok()?))
}
