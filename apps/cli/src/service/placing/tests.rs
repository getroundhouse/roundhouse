use std::ffi::OsString;
use std::path::Path;

use protocol::StampedBuild;
use protocol::paths::{ENV_HOME, HOME_VAR, Paths, resolve};

use super::placed::place_build;

fn paths_in(dir: &Path) -> Paths {
    let home = dir.join("home").display().to_string();
    let data = dir.join("data").display().to_string();
    resolve(|name| match name {
        HOME_VAR => Some(home.clone()),
        ENV_HOME => Some(data.clone()),
        _ => None,
    })
    .unwrap()
}

/// A build's programs, as files that say which build they are, in a directory of their own.
fn unpacked(dir: &Path, which: &str) -> std::path::PathBuf {
    let from = dir.join(which);
    std::fs::create_dir_all(&from).unwrap();
    for program in ["roundhoused", "roundhouse"] {
        std::fs::write(from.join(program), which).unwrap();
    }
    from
}

#[test]
fn placing_a_build_points_the_service_at_it_and_leaves_the_build_before_it_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let programs = [OsString::from("roundhoused"), OsString::from("roundhouse")];
    let first = StampedBuild::new("0.1.7", 7).unwrap();
    let second = StampedBuild::new("0.1.8", 8).unwrap();
    place_build(&paths, &first, &unpacked(dir.path(), "first"), &programs).unwrap();

    place_build(&paths, &second, &unpacked(dir.path(), "second"), &programs).unwrap();

    let current = paths.service_current();
    assert_eq!(std::fs::read_link(&current).unwrap(), Path::new(&second.dir_name()));
    for program in &programs {
        assert_eq!(std::fs::read_to_string(current.join(program)).unwrap(), "second");
        assert_eq!(std::fs::read_to_string(paths.service_build(&first).join(program)).unwrap(), "first");
    }
    let mut left: Vec<_> =
        std::fs::read_dir(paths.service_builds_dir()).unwrap().map(|e| e.unwrap().file_name()).collect();
    left.sort();
    let mut expected = vec![OsString::from("current"), first.dir_name().into(), second.dir_name().into()];
    expected.sort();
    assert_eq!(left, expected);
    assert!(paths.log.exists());
}

/// Only its user can open what it makes, as the rest of the hub's data directory.
#[cfg(unix)]
#[test]
fn a_placed_build_is_only_its_user_s() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let build = StampedBuild::new("0.1.7", 7).unwrap();

    place_build(&paths, &build, &unpacked(dir.path(), "first"), &[OsString::from("roundhoused")]).unwrap();

    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&paths.data_dir), 0o700);
    assert_eq!(mode(&paths.service_build(&build)), 0o700);
    assert_eq!(mode(&paths.log), 0o600);
}
