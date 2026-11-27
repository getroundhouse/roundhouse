//! The repository a test's hub works in, as the e2e and UI tests start it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::{git, kept};

/// The one this build of the test binary keeps, copied for each test, since making it takes three runs of git, which
/// were about a tenth of what a typical e2e test took. `None` when there's nowhere to keep it.
static KEPT: LazyLock<Option<PathBuf>> = LazyLock::new(|| kept("repository", make));

/// Makes `at` a repository on `main` with one commit, of `a.txt`, which says `one`, as if `git init`, `git add` and
/// `git commit` had made it there. It's a copy of one made once for this build of the binary, so its commit is the same
/// in every test, and was made when the first test of the build ran. The copy shares nothing with what it's copied
/// from, which no test changes.
///
/// # Panics
///
/// If `at` is there already, or can't be made.
pub fn repository_with_a_commit(at: &Path) {
    assert!(!at.exists(), "{} is there already", at.display());
    let made = match &*KEPT {
        Some(kept) => copy(kept, at).map_err(|e| format!("couldn't copy {} to {}: {e}", kept.display(), at.display())),
        None => make(at).map_err(|e| format!("couldn't make {}: {e}", at.display())),
    };
    made.unwrap_or_else(|why| panic!("{why}"));
}

fn make(at: &Path) -> io::Result<()> {
    fs::create_dir_all(at)?;
    git(at, &["init", "-q", "-b", "main"]);
    fs::write(at.join("a.txt"), "one\n")?;
    git(at, &["add", "."]);
    git(at, &["commit", "-qm", "first"]);
    Ok(())
}

/// Copies the directory `from` to `to`, and everything in it, as new files of `to`'s own, so nothing in one changes
/// with the other. A repository git made has no links.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let to = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_repository_is_one_commit_of_its_own_with_nothing_changed() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("first"), dir.path().join("second"));
        repository_with_a_commit(&first);
        repository_with_a_commit(&second);

        fs::write(first.join("a.txt"), "two\n").unwrap();
        git(&first, &["commit", "-qam", "second"]);

        assert!(KEPT.is_some(), "it's a copy of what's kept");
        assert_eq!(git(&second, &["log", "--format=%s %D"]), "first HEAD -> main");
        assert_eq!(fs::read_to_string(second.join("a.txt")).unwrap(), "one\n");
        assert_eq!(git(&second, &["status", "--porcelain"]), "", "a copy's files read as they were committed");
        assert_eq!(git(&first, &["log", "--format=%s"]), "second\nfirst");
    }
}
