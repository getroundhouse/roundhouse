//! A login shell that reads only the test's own profile, for a test's hub to ask, and to start a terminal's shell as.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `/bin/sh`, but as a login shell that reads `.profile` in its `HOME` and nothing of the machine's, such as
/// `/etc/profile`, where macOS's `path_helper` puts `/usr/local/bin` and Homebrew's folder ahead of the test's own,
/// so a real program there could be found before a test's stand-in. It takes out `-l` and `--login`, and passes the
/// rest on, as a terminal's shell or as `-i -c` for a command.
const LOGIN_SHELL: &str = r#"#!/bin/sh
[ -r "$HOME/.profile" ] && . "$HOME/.profile"
for arg do
  shift
  case "$arg" in -l|--login) ;; *) set -- "$@" "$arg" ;; esac
done
exec /bin/sh "$@"
"#;

/// The stand-in for a login shell, [`LOGIN_SHELL`], to give a test's hub as `SHELL`, so what it asks a login shell for,
/// and the shell a terminal starts, rest on nothing of the machine's setup.
///
/// It's [`kept`](super::kept) beside the test binary, as [`logged_out_gh`](super::logged_out_gh) is, and for the same
/// reasons.
///
/// # Panics
///
/// If there's nowhere beside the test binary to keep it, or it can't be written there.
pub fn login_shell() -> &'static Path {
    static SHELL: OnceLock<PathBuf> = OnceLock::new();
    SHELL.get_or_init(|| {
        let dir = super::kept("login-shell", make)
            .unwrap_or_else(|| panic!("couldn't keep a stand-in for a login shell beside the test binary"));
        dir.join("sh")
    })
}

/// Makes the folder at `dir`, with the stand-in in it as `sh`.
fn make(dir: &Path) -> io::Result<()> {
    fs::create_dir(dir)?;
    super::write_program(&dir.join("sh"), LOGIN_SHELL)
}

// The stand-in is a shell script, which only a Unix runs, as only a Unix has a login shell to ask.
#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;

    use super::*;

    /// Asked as the hub asks a login shell, it has what the profile in its home exports, and runs the command, with
    /// the `PATH` it was given, which a machine's `/etc/profile`, as macOS's is, would have changed.
    #[test]
    fn it_reads_only_the_profile_in_its_home_and_runs_what_its_asked() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".profile"), "export FROM_PROFILE=read\n").unwrap();

        let said = Command::new(login_shell())
            .args(["-i", "-l", "-c", "echo \"$FROM_PROFILE $PATH\""])
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();

        assert_eq!(String::from_utf8_lossy(&said.stdout), "read /usr/bin:/bin\n");
    }
}
