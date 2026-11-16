//! Running `gh`, as everything the worker asks of GitHub through it runs it.

use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use process::Environment;
use protocol::worker::GH_RUNS_WITHIN;
use tokio::process::Command;

/// Why `gh` didn't run to the end.
pub(crate) enum Unran {
    Missing,
    /// It couldn't start for another reason, or took too long.
    Failed(String),
}

/// Runs `gh` with `args` in `dir`, given `given`, and returns what it printed and how it exited, or why it didn't run
/// to the end, which it hasn't once it has taken [`GH_RUNS_WITHIN`]. It never prompts, since nobody would see it.
pub(crate) async fn run_gh(given: &Environment, dir: &Path, args: &[&str]) -> Result<Output, Unran> {
    run_gh_within(GH_RUNS_WITHIN, given, dir, args).await
}

/// [`run_gh`], with `within` in place of [`GH_RUNS_WITHIN`], for several runs that share it.
pub(crate) async fn run_gh_within(
    within: Duration,
    given: &Environment,
    dir: &Path,
    args: &[&str],
) -> Result<Output, Unran> {
    let mut command = Command::new("gh");
    command.args(args).current_dir(dir).env("GH_PROMPT_DISABLED", "1").stdin(Stdio::null()).kill_on_drop(true);
    given.apply(command.as_std_mut());
    match tokio::time::timeout(within, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Err(Unran::Missing),
        Ok(Err(e)) => Err(Unran::Failed(e.to_string())),
        Err(_) => Err(Unran::Failed(format!("gh took more than {within:?}"))),
    }
}
