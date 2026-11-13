//! A worktree's branches, read with git: which is checked out, and each branch the worktree has had checked out, with
//! its newest commit and where it's pushed. And where git writes them, which [`crate::branch_watch`] watches to tell
//! when to read them again.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use protocol::client::ApiError;
use protocol::model::{Branch, BranchName, Branches, CommitId, Head, RemoteName, Upstream};

use super::{commit_of, git, git_error, git_ok};

#[cfg(test)]
pub(crate) mod tests;

/// For each local branch, its name, newest commit and upstream, in fields git can't put in a name.
const LOCAL_FORMAT: &str = "%(refname)%00%(objectname)%00%(upstream)%00%(upstream:remotename)%00%(upstream:remoteref)";
/// For each upstream, its newest commit.
const TIP_FORMAT: &str = "%(refname)%00%(objectname)";

/// Where the worktree's git keeps what it says of its branches: its own directory, which has its `HEAD` and the
/// reflog of what it's had checked out, and the repository's, which has every branch, shared by all its worktrees.
/// Both as the filesystem names them, links followed, which is how it names a file that changes in them.
pub(crate) struct Places {
    git_dir: PathBuf,
    common_dir: PathBuf,
}

/// The branches, and the files of the refs they were read from, which a commit, a push or a fetch rewrites.
pub(crate) struct Read {
    pub(crate) branches: Branches,
    refs: Vec<PathBuf>,
}

/// A directory to watch, and whether what's in the directories inside it counts too.
pub(crate) struct Watched {
    pub(crate) directory: PathBuf,
    pub(crate) deep: bool,
}

pub(crate) async fn places(worktree: &Path) -> Result<Places, ApiError> {
    let found = git_ok(worktree, &["rev-parse", "--git-dir", "--git-common-dir"]).await?;
    let mut lines = found.lines();
    let (Some(git_dir), Some(common_dir)) = (lines.next(), lines.next()) else {
        return Err(git_error(format!("git didn't say where {}'s repository is", worktree.display())));
    };
    // Relative to the worktree when git prints them that way, and joining an absolute path replaces what's before it.
    let (git_dir, common_dir) = (worktree.join(git_dir), worktree.join(common_dir));
    let real = tokio::task::spawn_blocking(move || {
        Ok::<_, std::io::Error>((git_dir.canonicalize()?, common_dir.canonicalize()?))
    });
    match real.await {
        Ok(Ok((git_dir, common_dir))) => Ok(Places { git_dir, common_dir }),
        Ok(Err(e)) => Err(git_error(format!("finding {}'s repository: {e}", worktree.display()))),
        Err(e) => Err(git_error(format!("finding {}'s repository stopped: {e}", worktree.display()))),
    }
}

/// The branches, once, for a worktree nothing's watching yet.
pub(crate) async fn read_now(worktree: &Path) -> Result<Branches, ApiError> {
    Ok(read(worktree, &places(worktree).await?).await?.branches)
}

/// The branch checked out, if HEAD isn't detached.
pub(crate) async fn checked_out(worktree: &Path) -> Option<BranchName> {
    match head(worktree).await {
        Ok(Head::Branch { name }) => Some(name),
        Ok(Head::Detached { .. } | Head::Unknown) | Err(_) => None,
    }
}

pub(crate) async fn read(worktree: &Path, places: &Places) -> Result<Read, ApiError> {
    let head = head(worktree).await?;
    // Empty when the repository keeps no reflogs, and then only the branch checked out is known. So it is when git
    // can't read HEAD's reflog, as on a branch with no commits yet, which `git switch --orphan` makes, rather than
    // nothing being known.
    let reflog = git(worktree, &["reflog", "show", "--format=%gs", "HEAD", "--"]).await?;
    let mut names = if reflog.success { left(&reflog.stdout) } else { BTreeSet::new() };
    if let Head::Branch { name } = &head {
        names.insert(name.clone());
    }
    names.extend(rebasing(places).await);
    let format = format!("--format={LOCAL_FORMAT}");
    let listed = git_ok(worktree, &["for-each-ref", &format, "refs/heads/"]).await?;
    let had_it = |fields: &Vec<String>| {
        let name = fields.first().and_then(|name| name.strip_prefix("refs/heads/"));
        name.is_some_and(|name| names.contains(&BranchName::from(name)))
    };
    let local: Vec<Vec<String>> = fields(&listed).filter(had_it).collect();
    let upstreams: Vec<&str> =
        local.iter().filter_map(|fields| fields.get(2)).map(String::as_str).filter(|name| !name.is_empty()).collect();
    let tips = upstream_tips(worktree, &upstreams).await?;
    let each = local.iter().filter_map(|fields| branch(fields, &tips)).collect();
    let refs = local.iter().filter_map(|fields| fields.first()).map(String::as_str).chain(upstreams);
    // A part at a time, since `/` doesn't part a path that starts `\\?\`, as one on Windows does once its links are
    // followed.
    let refs = refs.map(|name| name.split('/').fold(places.common_dir.clone(), |file, part| file.join(part))).collect();
    Ok(Read { branches: Branches { head, had: each }, refs })
}

/// The directories git writes the branches in: the worktree's own, for its `HEAD` and reflog, kept as files or, in a
/// repository that keeps its refs in a reftable, in a reftable of its own; the repository's, for its config, which
/// says each branch's upstream, and the refs it packs into one file, but not what's in its directories, such as every
/// other worktree's and git's objects; and the repository's refs, kept as files or in its reftable.
pub(crate) fn watched(places: &Places) -> [Watched; 4] {
    let (git_dir, common_dir) = (&places.git_dir, &places.common_dir);
    [
        Watched { directory: git_dir.clone(), deep: true },
        Watched { directory: common_dir.clone(), deep: false },
        Watched { directory: common_dir.join("refs"), deep: true },
        Watched { directory: common_dir.join("reftable"), deep: false },
    ]
}

/// Whether git writing `file` may have changed what `read` found: the worktree's `HEAD` or reflog, the repository's
/// config or packed refs, a reftable, which holds every ref, or the file of a ref that was read, or a directory it's
/// in. Packing refs takes away the directories it empties, and git makes one again just before it writes a ref into
/// it, sooner than inotify can watch what's in it, so the directory being made may be all that's heard. Not the index,
/// which every `git status` in any worktree rewrites, nor another worktree's files, nor a branch the worktree hasn't
/// had, which only becomes one of its own by being checked out, which changes its `HEAD`.
pub(crate) fn matters(places: &Places, read: Option<&Read>, file: &Path) -> bool {
    if let Ok(own) = file.strip_prefix(&places.git_dir) {
        return own == Path::new("HEAD") || own == Path::new("logs").join("HEAD") || own.starts_with("reftable");
    }
    let Ok(shared) = file.strip_prefix(&places.common_dir) else { return false };
    let in_refs = shared.starts_with("refs") && shared != Path::new("refs");
    shared == Path::new("config")
        || shared == Path::new("packed-refs")
        || shared.starts_with("reftable")
        || read.is_some_and(|read| read.refs.iter().any(|read| read == file || (in_refs && read.starts_with(file))))
}

async fn head(worktree: &Path) -> Result<Head, ApiError> {
    let symbolic = git(worktree, &["symbolic-ref", "--quiet", "HEAD"]).await?;
    if symbolic.success {
        let target = symbolic.stdout.trim();
        let name = target.strip_prefix("refs/heads/").ok_or_else(|| git_error(format!("HEAD is {target}")))?;
        return Ok(Head::Branch { name: BranchName::from(name) });
    }
    // 1 is git's word for a HEAD that's detached, and anything else is a failure.
    if symbolic.code != Some(1) {
        return Err(git_error(format!("git couldn't say what's checked out: {}", symbolic.stderr)));
    }
    let commit = commit_of(worktree, "HEAD").await?.ok_or_else(|| git_error("HEAD is detached at no commit"))?;
    Ok(Head::Detached { commit })
}

/// The branch a rebase under way will go back to. HEAD is detached until it does, and nothing in the reflog names
/// that branch until then, unless the worktree left it before.
async fn rebasing(places: &Places) -> Vec<BranchName> {
    let notes = ["rebase-merge", "rebase-apply"].map(|under_way| places.git_dir.join(under_way).join("head-name"));
    let read = tokio::task::spawn_blocking(move || notes.map(|note| std::fs::read_to_string(note).unwrap_or_default()));
    let notes = read.await.unwrap_or_default();
    notes.iter().filter_map(|note| note.trim().strip_prefix("refs/heads/").map(BranchName::from)).collect()
}

/// Every branch the reflog's subjects say the worktree left, which covers every branch it's had checked out but the
/// one it has now. It names what it moved to as it was asked for, so `git checkout --detach main` says it moved to
/// `main`, but leaving a detached HEAD names its commit, so a branch only counts once it's been left. A commit has no
/// branch of its name, so [`read`] finds none for it.
fn left(subjects: &str) -> BTreeSet<BranchName> {
    let names = subjects.lines().filter_map(|subject| {
        let (from, _) = subject.strip_prefix("checkout: moving from ")?.split_once(" to ")?;
        Some(BranchName::from(from))
    });
    names.collect()
}

/// Each line's fields, in the order the format gives them.
fn fields(listed: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    listed.lines().map(|line| line.split('\0').map(str::to_string).collect())
}

/// The newest commit of each upstream, by its full ref name. A name is a pattern to git, which also matches the refs
/// below it, so only an exact match is kept. Nothing for none, which git would take as every ref.
async fn upstream_tips(worktree: &Path, upstreams: &[&str]) -> Result<HashMap<String, CommitId>, ApiError> {
    if upstreams.is_empty() {
        return Ok(HashMap::new());
    }
    let format = format!("--format={TIP_FORMAT}");
    let args: Vec<&str> = ["for-each-ref", format.as_str()].into_iter().chain(upstreams.iter().copied()).collect();
    let tips = fields(&git_ok(worktree, &args).await?)
        .filter_map(|fields| Some((fields.first()?.clone(), CommitId::from(fields.get(1)?.as_str()))))
        .filter(|(name, _)| upstreams.contains(&name.as_str()))
        .collect();
    Ok(tips)
}

/// A branch from what [`LOCAL_FORMAT`] gives it.
fn branch(fields: &[String], tips: &HashMap<String, CommitId>) -> Option<Branch> {
    let [refname, tip, upstream, remote, remote_ref] = fields else { return None };
    let upstream = (!upstream.is_empty() && !remote.is_empty() && !remote_ref.is_empty()).then(|| Upstream {
        remote: RemoteName::from(remote.as_str()),
        name: BranchName::from(remote_ref.strip_prefix("refs/heads/").unwrap_or(remote_ref)),
        tip: tips.get(upstream).cloned(),
    });
    let name = BranchName::from(refname.strip_prefix("refs/heads/")?);
    Some(Branch { name, tip: CommitId::from(tip.as_str()), upstream })
}
