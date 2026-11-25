//! The hub's observations, its copies of what others own, as it last observed them: one store for every kind, in memory
//! and in the cache file, [`crate::cache_file`], with a table and a version for each kind, so a build that changes one
//! kind's form empties only that kind. `docs/principles/observations.md` and `docs/principles/keeping-observations.md`
//! have the rules. The code that reads each kind fills it, as `super::workspace_lines` does a workspace's lines added
//! and removed.

mod changes;
mod file;
mod github_kinds;
mod hub_kinds;
mod kept;
mod kind;
mod kinds;
mod levels;
mod subjects;
mod tables;
#[cfg(test)]
mod tests;
mod workspace_kinds;
mod writer;

pub(crate) use file::PART;
pub(crate) use github_kinds::{
    GithubAppBot, GithubAppSaid, GithubProfile, GithubUsername, InstalledOn, RepositoryReach,
};
pub(crate) use hub_kinds::ReleaseOffered;
pub(crate) use kept::Observations;
pub(crate) use kinds::Kind;
pub(crate) use kinds::{
    AccountIdentity, AccountLimits, ModelCatalog, Referenced, SessionCache, SessionContext, SessionKey, SessionTodos,
    SessionToolServers, WaitKey, WaitRead, WorkspaceChanges, WorkspaceLines, WorkspacePullRequest,
    WorkspacePullRequestDetail, WorkspaceRepositoryUnseen,
};
pub(crate) use subjects::ChildWorktreeKey;
pub(crate) use workspace_kinds::{ChildWorktreeBranches, WorkspaceBranches, WorkspaceWorkerBuild};
