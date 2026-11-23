//! A test's directory in `/tmp`, and removing those that tests which died left there.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::ProcessRecord;

/// The file in a test's directory that says which test process made it.
const MADE_BY: &str = "made-by";

/// A new directory in `/tmp`, named from `prefix`, for a test whose hub's socket is in it, since a path under Cargo's
/// target directory can be longer than a socket's path can be. It says which process made it, so once that has gone
/// without removing it, as a test that's killed, interrupted or timed out goes, the next to make one from `prefix`
/// removes it. One that doesn't say, such as from a build before this, is left alone, since it can be a test's that's
/// still running, in another checkout.
///
/// # Panics
///
/// If it can't be made, which a test can't go on without.
pub fn tmp_dir(prefix: &str) -> TempDir {
    let tmp = Path::new("/tmp");
    remove_those_of_tests_gone(tmp, prefix);
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(tmp)
        .unwrap_or_else(|e| panic!("making a {prefix} directory: {e}"));
    ProcessRecord::write(&dir.path().join(MADE_BY), std::process::id())
        .unwrap_or_else(|e| panic!("saying which process made {}: {e}", dir.path().display()));
    dir
}

/// Removes each directory in `parent` named from `prefix` whose process has gone. One another test is removing at the
/// same time can fail to go, which the next try finishes.
fn remove_those_of_tests_gone(parent: &Path, prefix: &str) {
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let dir = entry.path();
        if ProcessRecord::read(&dir.join(MADE_BY)).is_some_and(|made_by| !made_by.is_running()) {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_whose_test_has_gone_is_removed_and_others_are_left() {
        let parent = tempfile::tempdir().unwrap();
        let in_parent = |name: &str| {
            let dir = parent.path().join(name);
            fs::create_dir(&dir).unwrap();
            dir
        };
        let gone = in_parent("t-gone");
        let record = ProcessRecord { pid: std::process::id(), started: "long ago".to_string() };
        fs::write(gone.join(MADE_BY), format!("{}\n{}\n", record.pid, record.started)).unwrap();
        let running = in_parent("t-running");
        ProcessRecord::write(&running.join(MADE_BY), std::process::id()).unwrap();
        let unsaid = in_parent("t-unsaid");
        let other = in_parent("other-gone");
        fs::copy(gone.join(MADE_BY), other.join(MADE_BY)).unwrap();

        remove_those_of_tests_gone(parent.path(), "t-");

        assert!(!gone.exists(), "its process has gone");
        assert!(running.exists(), "its process is this one");
        assert!(unsaid.exists(), "it doesn't say which process made it");
        assert!(other.exists(), "it isn't named from the prefix");
    }

    #[test]
    fn a_directory_made_says_this_process_made_it() {
        let dir = tmp_dir("process-tmp-dir-");

        let made_by = ProcessRecord::read(&dir.path().join(MADE_BY)).unwrap();

        assert_eq!(made_by.pid, std::process::id());
        assert!(made_by.is_running());
    }
}
