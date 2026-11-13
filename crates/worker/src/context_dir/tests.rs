use super::*;
use crate::git::tests::{changes, new_repo, run};

#[test]
fn what_an_agent_keeps_in_its_context_folder_never_shows_as_a_change() {
    let worktree = tempfile::tempdir().unwrap();
    run(worktree.path(), &["init", "-q"]);

    let context = Folder::Context.make(worktree.path()).unwrap();
    std::fs::create_dir(context.join("research")).unwrap();
    std::fs::write(context.join("research/notes.md"), "what I tried\n").unwrap();

    assert_eq!(changes(worktree.path()), "");
    assert_eq!(context, worktree.path().join(".context"));
}

/// Its own `.gitignore` then decides what's ignored in `.context/`, so a prompt's files rely on their folder's.
#[test]
fn a_prompt_s_files_stay_ignored_in_a_repository_with_a_gitignore_of_its_own_in_context() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::create_dir(repo.join(".context")).unwrap();
    std::fs::write(repo.join(".context/.gitignore"), "cache/\n").unwrap();
    run(&repo, &["add", ".context/.gitignore"]);
    run(&repo, &["commit", "-qm", "ignore the cache"]);

    Folder::Context.make(&repo).unwrap();
    let attachments = Folder::Attachments.make(&repo).unwrap();
    std::fs::write(attachments.join("build.log"), "failed\n").unwrap();

    assert_eq!(changes(&repo), "");
    assert_eq!(std::fs::read_to_string(repo.join(".context/.gitignore")).unwrap(), "cache/\n");
}

#[test]
fn a_folder_already_there_keeps_what_it_holds() {
    let worktree = tempfile::tempdir().unwrap();
    let context = worktree.path().join(Folder::Context.path());
    std::fs::create_dir_all(&context).unwrap();
    std::fs::write(context.join(".gitignore"), "notes.md\n").unwrap();
    std::fs::write(context.join("notes.md"), "what I tried\n").unwrap();

    Folder::Context.make(worktree.path()).unwrap();

    assert_eq!(std::fs::read_to_string(context.join(".gitignore")).unwrap(), "notes.md\n");
    assert_eq!(std::fs::read_to_string(context.join("notes.md")).unwrap(), "what I tried\n");
}

#[test]
fn a_link_where_a_folder_would_be_is_refused() {
    let (worktree, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), worktree.path().join(".context")).unwrap();

    let refused = Folder::Attachments.make(worktree.path()).unwrap_err();

    assert_eq!(refused.kind(), io::ErrorKind::NotADirectory);
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing was made through the link");
}
