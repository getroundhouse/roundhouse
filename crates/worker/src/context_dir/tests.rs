use super::*;
use crate::git::tests::{changes, run};

#[test]
fn what_an_agent_keeps_in_its_tmp_folder_never_shows_as_a_change() {
    let worktree = tempfile::tempdir().unwrap();
    run(worktree.path(), &["init", "-q"]);

    let tmp = Folder::Tmp.make(worktree.path()).unwrap();
    std::fs::write(tmp.join("notes.md"), "what I tried\n").unwrap();

    assert_eq!(tmp, worktree.path().join(".context/tmp"));
    assert_eq!(changes(worktree.path()), "");
}

#[test]
fn a_folder_already_there_keeps_what_it_holds() {
    let worktree = tempfile::tempdir().unwrap();
    let tmp = worktree.path().join(Folder::Tmp.path());
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join(".gitignore"), "notes.md\n").unwrap();
    std::fs::write(tmp.join("notes.md"), "what I tried\n").unwrap();

    Folder::Tmp.make(worktree.path()).unwrap();

    assert_eq!(std::fs::read_to_string(tmp.join(".gitignore")).unwrap(), "notes.md\n");
    assert_eq!(std::fs::read_to_string(tmp.join("notes.md")).unwrap(), "what I tried\n");
}

#[test]
fn a_link_where_a_folder_would_be_is_refused() {
    let (worktree, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), worktree.path().join(".context")).unwrap();

    let refused = Folder::Tmp.make(worktree.path()).unwrap_err();

    assert_eq!(refused.kind(), io::ErrorKind::NotADirectory);
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing was made through the link");
}
