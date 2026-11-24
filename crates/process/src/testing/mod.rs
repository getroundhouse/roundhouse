//! What tests of the crates that start processes share: building a program a test runs, such as the hub or a stand-in,
//! the whole environment a process a test starts runs in, git for a test's repositories and for the code under test,
//! the repository a test's hub works in, what a test binary keeps for every process of its build, a `gh` that's logged
//! in to nothing, a login shell that reads only the test's profile, putting a program where a test runs it, whether a
//! process is still running, and a test's directory in `/tmp`.

mod example;
mod git;
mod kept;
mod logged_out_gh;
mod login_shell;
mod program;
mod repository;
mod running;
#[cfg(unix)]
mod tmp_dir;
mod whole_environment;
mod workspace_build;

pub use example::Example;
pub use git::{git, git_environment, git_in};
pub use kept::kept;
pub use logged_out_gh::logged_out_gh;
pub use login_shell::login_shell;
pub use program::{install_program, write_program};
pub use repository::repository_with_a_commit;
pub use running::is_running;
#[cfg(unix)]
pub use tmp_dir::tmp_dir;
pub use whole_environment::WholeEnvironment;
pub use workspace_build::{once_per_run, the_hub_and_examples, workspace_build};
