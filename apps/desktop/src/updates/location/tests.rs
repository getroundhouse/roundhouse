use std::os::unix::fs::PermissionsExt as _;

use super::*;

/// An app in a folder of its own, which this user can change.
fn bundle_in_a_folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Roundhouse.app");
    std::fs::create_dir(&bundle).unwrap();
    (dir, bundle)
}

#[test]
fn an_app_it_can_replace_has_no_problem() {
    let (_dir, bundle) = bundle_in_a_folder();

    assert_eq!(cant_install(&bundle), None);
}

#[test]
fn an_app_macos_translocated_is_asked_to_move() {
    let bundle = PathBuf::from("/private/var/folders/xy/T/AppTranslocation/0A1B/d/Roundhouse.app");

    assert_eq!(cant_install(&bundle).map(|problem| problem.kind), Some(Kind::Translocated));
}

#[test]
fn an_app_in_a_folder_this_user_cant_change_says_so() {
    let (dir, bundle) = bundle_in_a_folder();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

    let problem = cant_install(&bundle);

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(problem.map(|problem| problem.kind), Some(Kind::ReadOnly));
}
