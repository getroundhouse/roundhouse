//! The `bash` a workspace's setup and archive scripts run in, which is the one they're written for: the system's on
//! Unix, and Git for Windows' on Windows.

#[cfg(any(windows, test))]
use std::path::Path;
use std::path::PathBuf;

use process::Environment;
use protocol::client::ApiError;
#[cfg(windows)]
use protocol::client::ErrorCode;

/// The system's own, which Unix finds on `PATH`, as the script finds the programs it runs.
#[cfg(unix)]
#[expect(clippy::unused_async, reason = "Windows' asks git where it's installed, which this takes the place of")]
pub(crate) async fn find(_given: &Environment) -> Result<PathBuf, ApiError> {
    Ok(PathBuf::from("bash"))
}

/// Git for Windows' own, beside the git the worker runs, by its whole path rather than by its name. For a program
/// whose command doesn't set `PATH`, as the worker's don't, Windows looks in System32 before `PATH`, and System32 has
/// WSL's `bash`, which runs a script in Linux, without the workspace's variables and with the worktree at another
/// path. A git with no `bash` beside it, as Git for Windows' smallest edition has none, fails the script, saying why.
#[cfg(windows)]
pub(crate) async fn find(given: &Environment) -> Result<PathBuf, ApiError> {
    let exec_path = crate::git::exec_path(given)
        .await
        .map_err(|e| missing(&format!("git couldn't say where it's installed: {}", e.message)))?;
    beside_git(&exec_path).ok_or_else(|| missing(&format!("there's none beside the git at {}", exec_path.display())))
}

/// How many folders above git's programs the folder Git for Windows is installed in can be: three, above
/// `mingw64\libexec\git-core`, or `clangarm64` in place of `mingw64` on ARM64. No further, so a `bash` that happens to
/// be in a folder above that isn't taken for Git's.
#[cfg(any(windows, test))]
const INSTALLED_AT_MOST_ABOVE: usize = 3;

/// Git for Windows' `bash` for the git whose programs are at `exec_path`, as `git --exec-path` says, which is in a
/// folder inside the one Git is installed in. It's the `bash` in `bin`, which puts Git's own programs, such as `touch`
/// and `sleep`, first on the script's `PATH`, as Git Bash does. The one in `usr\bin` doesn't, so a script it runs on
/// Windows' own `PATH` has none of them.
#[cfg(any(windows, test))]
fn beside_git(exec_path: &Path) -> Option<PathBuf> {
    let folders = exec_path.ancestors().take(INSTALLED_AT_MOST_ABOVE + 1);
    folders.map(|folder| folder.join("bin").join("bash.exe")).find(|bash| bash.is_file())
}

#[cfg(windows)]
fn missing(why: &str) -> ApiError {
    let message = format!("Scripts run in Git for Windows' bash, and {why}. Install Git for Windows with its bash.");
    ApiError::new(ErrorCode::Unavailable, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn gits_bash_is_the_launcher_in_the_folder_git_is_installed_in() {
        let above = tempfile::tempdir().unwrap();
        let git = above.path().join("Git");
        let exec_path = git.join("mingw64").join("libexec").join("git-core");
        std::fs::create_dir_all(&exec_path).unwrap();
        touch(&git.join("usr").join("bin").join("bash.exe"));
        touch(&above.path().join("bin").join("bash.exe"));

        assert_eq!(beside_git(&exec_path), None, "neither the bash that leaves PATH alone, nor one above Git's folder");

        let launcher = git.join("bin").join("bash.exe");
        touch(&launcher);

        assert_eq!(beside_git(&exec_path), Some(launcher));
    }
}
