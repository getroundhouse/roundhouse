//! What a test binary makes once for every process of its build, and keeps beside itself.
//!
//! nextest, which CI runs the tests with, starts a process for each test, so what each process made for itself would be
//! made for every test. Kept beside the binary, named for its build, it's made by the first process of a build, and the
//! rest find it there.

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

/// What this build of the running test binary keeps as `what`, a file or a directory, which `make` makes at the path
/// it's given, if no process of this build has kept it yet. `None` when there's nowhere beside the binary to keep it,
/// such as when `make` can't write there, and the caller makes its own.
///
/// Its name says the binary's size and when it was written, which a build that changes anything changes, so a build
/// never finds what another made. It's made beside where it's kept, and renamed into place, so another process finds
/// all of it or none, and once it's there, what earlier builds kept is deleted. What's kept is only read, and never
/// changed, since every process of the build shares it. A `make` that panics leaves nothing, and panics here.
pub fn kept(what: &str, make: impl FnOnce(&Path) -> io::Result<()>) -> Option<PathBuf> {
    let (dir, binary, build) = this_build()?;
    let kept = dir.join(format!("{binary}.{build}.{what}"));
    if kept.exists() {
        return Some(kept);
    }
    let making = Making(dir.join(format!("{binary}.{build}.{}.{what}.making", std::process::id())));
    make(&making.0).ok()?;
    // Another process of the build may have kept it first, which, made the same way, is the same.
    if fs::rename(&making.0, &kept).is_err() && !kept.exists() {
        return None;
    }
    let_go_of_earlier(&dir, &binary, &build, what);
    Some(kept)
}

/// The directory the running binary is in, its name, and a name for its build. `None` when it can't be found.
fn this_build() -> Option<(PathBuf, String, String)> {
    let binary = std::env::current_exe().ok()?;
    let written = fs::metadata(&binary).ok()?;
    let mut build = DefaultHasher::new();
    (written.len(), written.modified().ok()?).hash(&mut build);
    let name = binary.file_stem()?.to_str()?.to_string();
    Some((binary.parent()?.to_path_buf(), name, format!("{:016x}", build.finish())))
}

/// Deletes what earlier builds of `binary` kept as `what`, and what they were making when they stopped.
fn let_go_of_earlier(dir: &Path, binary: &str, build: &str, what: &str) {
    let ours = format!("{binary}.{build}.");
    let making = format!(".{what}.making");
    let kept_as_what = |name: &str| {
        let Some(rest) = name.strip_prefix(binary).and_then(|rest| rest.strip_prefix('.')) else { return false };
        rest.split_once('.').is_some_and(|(_, kept)| kept == what || kept.ends_with(&making))
    };
    let earlier = fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|other| {
        let name = other.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        kept_as_what(name) && !name.starts_with(&ours)
    });
    for other in earlier {
        remove(&other);
    }
}

/// Removes a file or a directory, if it's there. One that can't be removed only stays.
fn remove(path: &Path) {
    let _ = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
}

/// What's being made, removed if it's still there once making it has ended, as when `make` panics or fails, or another
/// process kept its own first.
struct Making(PathBuf);

impl Drop for Making {
    fn drop(&mut self) {
        remove(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// Each test keeps what it's named for, so tests of this run side by side don't find what another made.
    #[test]
    fn what_is_kept_is_made_once_whole_and_found_by_the_next_to_ask() {
        let what = "kept-test-made-once";
        let_go_of_ours(what);
        let makes = Cell::new(0);
        let make = |path: &Path| {
            makes.set(makes.get() + 1);
            fs::create_dir(path)?;
            fs::write(path.join("inside"), "whole")
        };

        let first = kept(what, make).unwrap();
        let second = kept(what, make).unwrap();

        assert_eq!((&first, makes.get()), (&second, 1));
        assert_eq!(fs::read_to_string(first.join("inside")).unwrap(), "whole");
        let (dir, _, build) = this_build().unwrap();
        assert!(first.starts_with(&dir) && first.to_string_lossy().contains(&build), "{}", first.display());
        assert_eq!(left_beside_the_binary(&format!("{what}.making")), Vec::<String>::new());
    }

    #[test]
    fn what_earlier_builds_kept_goes_once_this_build_keeps_its_own() {
        let what = "kept-test-earlier";
        let_go_of_ours(what);
        let (dir, binary, _) = this_build().unwrap();
        let earlier = dir.join(format!("{binary}.0000000000000000.{what}"));
        let left_making = dir.join(format!("{binary}.0000000000000000.1.{what}.making"));
        let another = dir.join(format!("{binary}.0000000000000000.kept-test-another"));
        for path in [&earlier, &left_making, &another] {
            fs::write(path, "earlier").unwrap();
        }

        let ours = kept(what, |path| fs::write(path, "ours")).unwrap();

        assert_eq!(fs::read_to_string(ours).unwrap(), "ours");
        assert!(!earlier.exists() && !left_making.exists());
        assert!(another.exists(), "only what's kept as the same thing goes");
        fs::remove_file(another).unwrap();
    }

    #[test]
    fn a_make_that_panics_keeps_nothing() {
        let what = "kept-test-panics";

        let panicked = std::panic::catch_unwind(|| {
            kept(what, |path| {
                fs::write(path, "half")?;
                panic!("a migration failed");
            })
        });

        assert!(panicked.is_err());
        assert_eq!(left_beside_the_binary(what), Vec::<String>::new());
    }

    #[test]
    fn a_make_that_fails_keeps_nothing_and_leaves_the_caller_to_make_its_own() {
        let what = "kept-test-fails";

        let found = kept(what, |path| {
            fs::write(path, "half")?;
            Err(io::Error::other("the disk is full"))
        });

        assert_eq!(found, None);
        assert_eq!(left_beside_the_binary(what), Vec::<String>::new());
    }

    /// What this build kept as `what` in an earlier run of its tests, which a test of making it again would find.
    fn let_go_of_ours(what: &str) {
        let (dir, binary, build) = this_build().unwrap();
        remove(&dir.join(format!("{binary}.{build}.{what}")));
    }

    /// The names beside the binary that are this binary's and say `what`.
    fn left_beside_the_binary(what: &str) -> Vec<String> {
        let (dir, binary, _) = this_build().unwrap();
        let names = fs::read_dir(dir).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned());
        names.filter(|name| name.starts_with(&binary) && name.contains(what)).collect()
    }
}
