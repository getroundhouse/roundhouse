//! A `gh` that's logged in to nothing, for the processes a test starts to find ahead of the person's own.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `gh` as it is before `gh auth login`, which is what the person's own printed in a home of the tests' own: it names
/// no login for github.com, and refuses everything else.
const LOGGED_OUT_GH: &str = r#"#!/bin/sh
if [ "$1 $2" = 'auth status' ]; then
  echo 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2
  exec echo '{"hosts":{}}'
fi
echo 'To get started with GitHub CLI, please run:  gh auth login' >&2
exit 4
"#;

/// The folder that holds [`LOGGED_OUT_GH`] as `gh`, to put first on the `PATH` of what a test starts, so what a hub
/// or a worker hears of `gh` doesn't turn on whether the person has one, or which. A test that has `gh` answer puts
/// its own ahead of this one.
///
/// It's [`kept`](super::kept) beside the test binary, so every process of a build shares it, and finds all of it or
/// none.
///
/// # Panics
///
/// If there's nowhere beside the test binary to keep it, or it can't be written there.
pub fn logged_out_gh() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        super::kept("logged-out-gh", make)
            .unwrap_or_else(|| panic!("couldn't keep a logged-out stand-in for gh beside the test binary"))
    })
}

/// Makes the folder at `bin`, with the stand-in in it as `gh`.
fn make(bin: &Path) -> io::Result<()> {
    fs::create_dir(bin)?;
    super::write_program(&bin.join("gh"), LOGGED_OUT_GH)
}

// The stand-in is a shell script, which only a Unix runs, as only a Unix has the hubs and workers that find it.
#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn it_says_its_logged_in_to_nothing_and_refuses_the_rest() {
        let gh = logged_out_gh().join("gh");

        let status = Command::new(&gh).args(["auth", "status", "--json", "hosts"]).output().unwrap();
        let token = Command::new(&gh).args(["auth", "token"]).output().unwrap();

        assert!(status.status.success());
        assert_eq!(String::from_utf8_lossy(&status.stdout), "{\"hosts\":{}}\n");
        assert_eq!(token.status.code(), Some(4));
        assert_eq!(fs::read_dir(logged_out_gh()).unwrap().count(), 1, "only gh is left in its folder");
    }
}
