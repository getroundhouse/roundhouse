//! What `git ls-files` says is in a worktree: the entries git tracks and what kind each is, new files, ignored ones,
//! and tracked ones deleted since. `crate::files` makes All files' list from them.

use std::collections::HashSet;
use std::path::Path;

use protocol::client::ApiError;

use super::{git_error, run};

/// What kind of entry git tracks at a path, from its mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tracked {
    File,
    /// A submodule: a repository of its own, which git tracks as one commit rather than by its files.
    Submodule,
    /// A link, which may lead to a folder.
    Link,
}

pub(crate) struct Entries {
    /// Every entry git tracks, once each, even one with conflicts, which git lists once for each side.
    pub(crate) tracked: Vec<(String, Tracked)>,
    /// New files git doesn't ignore.
    pub(crate) untracked: Vec<String>,
    /// Files git ignores, and folders it ignores whole, which end with a slash.
    pub(crate) ignored: Vec<String>,
    /// Entries git tracks that have been deleted from the worktree.
    pub(crate) deleted: HashSet<String>,
}

/// Lists the worktree at `worktree` four ways at once, since each walks it. A path that isn't UTF-8 is left out, since
/// a client couldn't name it to ask for it.
pub(crate) async fn entries(worktree: &Path) -> Result<Entries, ApiError> {
    let (tracked, untracked, ignored, deleted) = tokio::try_join!(
        listed(worktree, &["ls-files", "-z", "--stage"]),
        listed(worktree, &["ls-files", "-z", "--others", "--exclude-standard"]),
        listed(worktree, &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"]),
        listed(worktree, &["ls-files", "-z", "--deleted"]),
    )?;
    let mut seen = HashSet::new();
    let tracked =
        tracked.iter().filter_map(|line| staged(line)).filter(|(path, _)| seen.insert(path.clone())).collect();
    Ok(Entries { tracked, untracked, ignored, deleted: deleted.into_iter().collect() })
}

/// An entry of `ls-files --stage`, which is its mode, its object, its stage and a tab, then its path.
fn staged(line: &str) -> Option<(String, Tracked)> {
    let (about, path) = line.split_once('\t')?;
    let kind = match about.split(' ').next()? {
        "160000" => Tracked::Submodule,
        "120000" => Tracked::Link,
        _ => Tracked::File,
    };
    Some((path.to_string(), kind))
}

/// What git prints, as one entry for each NUL it ends with.
async fn listed(worktree: &Path, args: &[&str]) -> Result<Vec<String>, ApiError> {
    let output = run(worktree, args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(git_error(format!("git {} failed: {}", args.join(" "), stderr.trim())));
    }
    let entries = output.stdout.split(|byte| *byte == 0).filter(|entry| !entry.is_empty());
    Ok(entries.filter_map(|entry| std::str::from_utf8(entry).ok()).map(str::to_string).collect())
}
