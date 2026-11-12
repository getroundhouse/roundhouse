use super::*;
use crate::git::tests::{changes, run};

#[test]
fn a_file_goes_in_the_attachments_folder_and_git_never_sees_it() {
    let worktree = tempfile::tempdir().unwrap();
    run(worktree.path(), &["init", "-q"]);

    let path = write(worktree.path(), "build.log", b"error at line 42\n").unwrap();

    assert_eq!(path, worktree.path().join(".context/attachments/build.log"));
    assert_eq!(std::fs::read(&path).unwrap(), b"error at line 42\n");
    assert_eq!(changes(worktree.path()), "");
    let mut there: Vec<_> =
        std::fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    there.sort();
    assert_eq!(there, [".gitignore", "build.log"], "nothing partial is left behind");
}

#[test]
fn the_same_file_again_is_the_same_file_and_another_with_its_name_is_numbered() {
    let worktree = tempfile::tempdir().unwrap();

    let first = write(worktree.path(), "build.log", b"one").unwrap();
    let again = write(worktree.path(), "build.log", b"one").unwrap();
    let other = write(worktree.path(), "build.log", b"two").unwrap();
    let third = write(worktree.path(), "build.log", b"three").unwrap();

    assert_eq!(again, first);
    assert_eq!(other.file_name().unwrap(), "build (2).log");
    assert_eq!(third.file_name().unwrap(), "build (3).log");
    assert_eq!(std::fs::read(&first).unwrap(), b"one", "a file already there is never replaced");
    assert_eq!(numbered(".env", 2), ".env (2)");
    assert_eq!(numbered("Makefile", 3), "Makefile (3)");
}

#[test]
fn a_name_cant_put_a_file_outside_the_folder() {
    let worktree = tempfile::tempdir().unwrap();

    let path = write(worktree.path(), "../../escaped.txt", b"x").unwrap();

    assert_eq!(path, worktree.path().join(".context/attachments/escaped.txt"));
}

#[test]
fn a_folder_with_the_files_name_doesnt_stop_it() {
    let worktree = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(worktree.path().join(".context/attachments/data.csv")).unwrap();

    let path = write(worktree.path(), "data.csv", b"a,b\n").unwrap();

    assert_eq!(path.file_name().unwrap(), "data (2).csv");
}
