use std::fs;
use std::io::Write as _;

use super::{create_dir_all, file_options, restrict};

#[test]
fn a_directory_is_made_with_what_is_missing_above_it() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("above").join("dir");

    create_dir_all(&dir).unwrap();

    assert!(dir.is_dir());
}

#[test]
fn a_directory_there_already_is_left_with_what_is_in_it() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("kept"), "kept").unwrap();

    create_dir_all(root.path()).unwrap();

    assert_eq!(fs::read_to_string(root.path().join("kept")).unwrap(), "kept");
}

#[test]
fn a_file_is_opened_as_the_caller_asks() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");

    file_options().write(true).create_new(true).open(&path).unwrap().write_all(b"written").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"written");
    assert!(file_options().write(true).create_new(true).open(&path).is_err(), "create_new opened a file that's there");
}

#[test]
fn a_file_restricted_keeps_what_is_in_it() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");
    fs::write(&path, "kept").unwrap();

    restrict(&path).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "kept");
}

/// The modes, which only Unix has.
#[cfg(unix)]
mod modes {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use super::super::{create_dir_all, file_options, restrict};

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_directory_made_and_what_is_made_above_it_are_only_this_users() {
        let root = tempfile::tempdir().unwrap();
        let above = root.path().join("above");

        create_dir_all(&above.join("dir")).unwrap();

        assert_eq!(mode(&above), 0o700);
        assert_eq!(mode(&above.join("dir")), 0o700);
    }

    #[test]
    fn a_directory_there_already_keeps_its_mode() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), Permissions::from_mode(0o755)).unwrap();

        create_dir_all(root.path()).unwrap();

        assert_eq!(mode(root.path()), 0o755);
    }

    #[test]
    fn a_file_the_options_make_is_only_this_users() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");

        file_options().write(true).create_new(true).open(&path).unwrap();

        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_file_there_already_keeps_its_mode_when_the_options_open_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();

        file_options().append(true).create(true).open(&path).unwrap();

        assert_eq!(mode(&path), 0o644);
    }

    #[test]
    fn a_file_restricted_is_only_this_users() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();

        restrict(&path).unwrap();

        assert_eq!(mode(&path), 0o600);
    }
}
