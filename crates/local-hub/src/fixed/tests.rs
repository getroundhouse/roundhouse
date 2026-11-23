use super::{place, remove_leftovers};
use process::RunningFrom;
use protocol::paths::Paths;
use std::path::PathBuf;
use std::time::Duration;

use super::super::tests::{build, files_in, paths_in};

/// Puts a build's copy in place, holding `contents`, as if that build had started a hub here.
fn copy_of(paths: &Paths, number: u64, contents: &str) -> PathBuf {
    let copy = paths.hub_copy(&build(number));
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, contents).unwrap();
    copy
}

/// The names a copy was linked to on its way to the fixed path.
fn linking(paths: &Paths) -> Vec<String> {
    let prefix = protocol::paths::linking_hub_prefix();
    files_in(&paths.daemons_dir).into_iter().filter(|name| name.starts_with(&prefix)).collect()
}

#[test]
fn a_hub_is_started_from_one_path_whichever_build_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let first = copy_of(&paths, 1, "first");
    let second = copy_of(&paths, 2, "second");

    let placed_first = place(&paths, &first).unwrap();
    let held_first = std::fs::read_to_string(&placed_first).unwrap();
    let placed_second = place(&paths, &second).unwrap();

    assert_eq!(placed_first, paths.fixed_hub());
    assert_eq!(placed_second, paths.fixed_hub());
    assert_eq!(held_first, "first");
    assert_eq!(std::fs::read_to_string(paths.fixed_hub()).unwrap(), "second");
    assert_eq!(linking(&paths), Vec::<String>::new());
}

/// What was at the fixed path is never written to, only replaced, so a hub running from it goes on running its own
/// build, and so does its build's copy, which is the same file.
#[test]
fn what_was_at_the_fixed_path_stays_its_builds_copy_whole() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let first = copy_of(&paths, 1, "first");
    place(&paths, &first).unwrap();
    // The first build's hub, running from the fixed path.
    let running = RunningFrom::mark(&paths.fixed_hub()).unwrap();

    place(&paths, &copy_of(&paths, 2, "second")).unwrap();

    assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
    // The mark the hub made through the fixed path is on its build's copy.
    assert!(process::claim_unused(&first).unwrap().is_none());
    drop(running);
}

/// A mark goes only once every process holding it lets go, and a child another test is starting holds what this
/// process has open until it runs its program, so the link goes by a later look, as a later client's start would
/// remove it.
#[test]
fn a_link_left_on_the_way_goes_once_nothing_runs_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let first = copy_of(&paths, 1, "first");
    let second = copy_of(&paths, 2, "second");
    // As a client killed between linking and renaming leaves them, one of them run from.
    let (used, unused) = (paths.linking_hub(1), paths.linking_hub(2));
    std::fs::hard_link(&first, &used).unwrap();
    std::fs::hard_link(&second, &unused).unwrap();
    let running = RunningFrom::mark(&used).unwrap();

    remove_leftovers(&paths);
    let while_running = linking(&paths);
    drop(running);
    waiting::thread::until(Duration::from_secs(10), "the link nothing runs from now to go", || {
        remove_leftovers(&paths);
        linking(&paths).is_empty()
    });

    let used_name = used.file_name().unwrap().to_str().unwrap().to_owned();
    assert_eq!(while_running, vec![used_name]);
    // Only the names go, and each build's copy stays.
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
}
