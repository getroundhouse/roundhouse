//! What tests of the crates that start processes share: building a program a test runs, the whole environment a
//! process a test starts runs in, git for a test's repositories and for the code under test, the repository a test's
//! hub works in, what a test binary keeps for every process of its build, a `gh` that's logged in to nothing, and
//! whether a process is still running.

mod git;
mod kept;
mod logged_out_gh;
mod repository;
mod running;
mod whole_environment;
mod workspace_build;

pub use git::{git, git_environment, git_in};
pub use kept::kept;
pub use logged_out_gh::logged_out_gh;
pub use repository::repository_with_a_commit;
pub use running::is_running;
pub use whole_environment::WholeEnvironment;
pub use workspace_build::workspace_build;
