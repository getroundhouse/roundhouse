use std::fs::File;

use process::RunningFrom;

use protocol::paths::daemon_file;

use super::super::tests::{build, copy_name, files_in, paths_in};
use super::*;

/// Longer ago than a copy is kept.
const LONG_AGO: Duration = Duration::from_hours(48);

/// Puts a copy in place for each build numbered, as if each had started a hub here, `ago`.
fn copies(paths: &Paths, numbers: &[u64], ago: Duration) {
    for &number in numbers {
        copy_at(&paths.hub_copy(&build(number)), ago);
    }
}

/// Puts a copy in place at `copy`, made `ago`.
fn copy_at(copy: &Path, ago: Duration) {
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    File::create(copy).unwrap().set_modified(SystemTime::now() - ago).unwrap();
}

/// What's left in the directory of copies.
fn left(paths: &Paths) -> Vec<String> {
    files_in(&paths.daemons_dir)
}

/// A client of an older build leaves a newer one's recent copy, as it does any recent copy.
#[test]
fn every_copy_goes_once_a_day_old_but_this_builds() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[1, 2, 4], LONG_AGO);
    copies(&paths, &[3], Duration::from_hours(1));

    remove_unused(&paths, &build(2));

    assert_eq!(left(&paths), ["abc123-2", "abc123-3"]);
}

/// Such as a worker that outlived the hub of its build.
#[test]
fn a_copy_a_process_runs_from_stays() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[1, 2, 3], LONG_AGO);
    let _running = RunningFrom::mark(&paths.hub_copy(&build(1))).unwrap();

    remove_unused(&paths, &build(3));

    assert_eq!(left(&paths), ["abc123-1", "abc123-3"]);
}

/// Builds from before copies were called Roundhouse Hub named theirs for the daemon, and a worker of one of them can
/// still be running from it.
#[test]
fn a_copy_named_for_the_daemon_goes_unless_a_process_runs_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[3], LONG_AGO);
    let named_for_the_daemon = |number| paths.hub_copy(&build(number)).with_file_name(daemon_file());
    for number in [1, 2] {
        copy_at(&named_for_the_daemon(number), LONG_AGO);
    }
    let _running = RunningFrom::mark(&named_for_the_daemon(1)).unwrap();

    remove_unused(&paths, &build(3));

    assert_eq!(left(&paths), ["abc123-1", "abc123-3"]);
    assert_eq!(files_in(named_for_the_daemon(1).parent().unwrap()), [daemon_file()]);
}

#[test]
fn a_temporary_copy_goes_once_abandoned_and_one_being_made_stays() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[1], LONG_AGO);
    let build_dir = paths.hub_copy(&build(1)).parent().unwrap().to_path_buf();
    let (abandoned, being_made) = (format!("{}10.0", temporary_prefix()), format!("{}20.0", temporary_prefix()));
    let file = File::create(build_dir.join(&abandoned)).unwrap();
    file.set_modified(SystemTime::now() - ABANDONED_AFTER - Duration::from_secs(1)).unwrap();
    File::create(build_dir.join(&being_made)).unwrap();

    remove_unused(&paths, &build(1));

    assert_eq!(files_in(&build_dir), [being_made, copy_name(&paths)]);
}

/// A client killed while making the first copy of its build can leave a directory with nothing else in it.
#[test]
fn a_directory_with_no_copy_goes_unless_one_is_being_made_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[3], LONG_AGO);
    for number in [1, 2] {
        std::fs::create_dir_all(paths.hub_copy(&build(number)).parent().unwrap()).unwrap();
    }
    let making = paths.hub_copy(&build(2)).with_file_name(format!("{}10.0", temporary_prefix()));
    File::create(making).unwrap();

    remove_unused(&paths, &build(3));

    assert_eq!(left(&paths), ["abc123-2", "abc123-3"]);
}

#[test]
fn what_is_not_a_builds_directory_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    copies(&paths, &[1, 2], LONG_AGO);
    for other in ["notes", "abc123-07"] {
        std::fs::create_dir_all(paths.daemons_dir.join(other)).unwrap();
    }
    std::fs::write(paths.daemons_dir.join("abc123-9"), "a file, not a directory").unwrap();

    remove_unused(&paths, &build(2));

    assert_eq!(left(&paths), ["abc123-07", "abc123-2", "abc123-9", "notes"]);
}
