use std::os::unix::fs::PermissionsExt as _;

use super::*;

#[test]
fn a_tree_with_read_only_directories_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("alder-1");
    let cache = tree.join("cache/pkg");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("module.go"), "package pkg\n").unwrap();
    for read_only in [cache.as_path(), cache.parent().unwrap()] {
        std::fs::set_permissions(read_only, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let failed = delete_trees(std::slice::from_ref(&tree));

    assert!(failed.is_empty(), "{failed:?}");
    assert!(!tree.exists());
}

#[test]
fn a_tree_already_gone_is_no_failure_and_the_rest_are_still_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, there) = (dir.path().join("gone"), dir.path().join("there"));
    std::fs::create_dir_all(there.join("target")).unwrap();

    let failed = delete_trees(&[gone, there.clone()]);

    assert!(failed.is_empty(), "{failed:?}");
    assert!(!there.exists());
}

#[test]
fn a_link_out_of_the_tree_is_deleted_without_touching_where_it_points() {
    let dir = tempfile::tempdir().unwrap();
    let (tree, outside) = (dir.path().join("alder-1"), dir.path().join("outside"));
    std::fs::create_dir_all(outside.join("kept")).unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o555)).unwrap();
    let read_only = tree.join("read-only");
    std::fs::create_dir_all(&read_only).unwrap();
    std::os::unix::fs::symlink(&outside, read_only.join("link")).unwrap();
    std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o555)).unwrap();

    let failed = delete_trees(std::slice::from_ref(&tree));

    assert!(failed.is_empty(), "{failed:?}");
    assert!(!tree.exists());
    assert!(outside.join("kept").exists());
    let mode = std::fs::metadata(&outside).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o555, "where the link points is left as it was");
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
}
