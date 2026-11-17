//! What tests of the crates that start processes share: building a program a test runs, the whole environment a
//! process a test starts runs in, git for a test's repositories and for the code under test, and whether a process is
//! still running.

mod git;
mod running;
mod whole_environment;
mod workspace_build;

pub use git::{git, git_environment, git_in};
pub use running::is_running;
pub use whole_environment::WholeEnvironment;
pub use workspace_build::workspace_build;
