//! Making, setting up, archiving and putting back a workspace through the worker's requests.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::model::{BranchName, Head};
use protocol::worker::{
    ArchiveScript, CaptureArchive, NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace, RepoSource,
    Request as _, RestoreWorkspace, SetupDone, SetupWorkspace, WorkerResponse,
};

use super::super::add_child;
use super::support::{added, config, served};
use crate::git::tests::{new_repo, run};
use crate::serve::{Worker, handle};

/// The ignored files setup copies are copied into a new one, and into one asked for again where it's missing them,
/// which leaves what's there as it is.
#[tokio::test]
async fn ignored_files_are_copied_into_a_new_one_and_again_only_where_they_re_missing() {
    let served = served().await;
    let state = &served.worker.state;
    let env = Path::new(&served.child()).join(".env");

    let first = added(add_child(state, served.adding()).await);
    let copied_at_first = std::fs::read_to_string(&env).unwrap();
    std::fs::write(&env, "SECRET=2\n").unwrap();
    let kept = added(add_child(state, served.adding()).await);
    std::fs::remove_file(&env).unwrap();
    let again = added(add_child(state, served.adding()).await);

    assert_eq!((first.copied, copied_at_first.as_str()), (vec![".env".to_string()], "SECRET=1\n"));
    assert_eq!(first.branches.unwrap().head, Head::Branch { name: BranchName::from("alder/try") });
    assert!(kept.copied.is_empty(), "what was there was left: {:?}", kept.copied);
    assert_eq!(again.copied, [".env"], "copied again once it was missing");
}

/// An archive script runs in the worktree, and one that fails refuses the archive as what forcing goes past, so the
/// hub leaves the workspace as it was unless it's forced.
#[tokio::test]
async fn an_archive_script_that_fails_refuses_the_archive_and_one_that_passes_lets_it_go_on() {
    for (exits, refused) in [(0, None), (1, Some(ErrorCode::NeedsForce))] {
        let served = served().await;
        let worker = &served.worker;
        let worktree = Path::new(&served.own);
        std::fs::create_dir_all(worktree.join(".conductor")).unwrap();
        let settings = format!("[scripts]\narchive = \"touch ran && exit {exits}\"\n");
        std::fs::write(worktree.join(".conductor/settings.toml"), settings).unwrap();
        let script = ArchiveScript { source: served.source.clone(), path: served.own.clone(), env: vec![] };

        let answer = handle(&worker.state, &worker.forwarders, Ok(script.into_request())).await;

        assert_eq!(answer.as_ref().err().map(|e| e.code), refused, "exiting {exits}: {answer:?}");
        assert!(worktree.join("ran").exists(), "exiting {exits}: it didn't run in the worktree");
    }
}

/// A workspace whose repository is given by its address is named, made, set up, archived and put back through the
/// worker's requests as one from a checkout is, from the one clone the first of them makes, which has no local files
/// to copy.
#[tokio::test]
async fn a_workspace_made_from_a_clone_is_archived_and_put_back_through_the_worker_s_requests() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    run(dir.path(), &["clone", "-q", "--bare", new_repo(dir.path()).to_str().unwrap(), remote.to_str().unwrap()]);
    let worker = Worker::new(config(dir.path()));
    let ask = |request| handle(&worker.state, &worker.forwarders, Ok(request));
    let (source, branch) = (RepoSource::Clone { url: remote.to_string_lossy().into_owned() }, "alder/start");
    let making = PrepareWorkspace {
        repo_name: "repo".into(),
        source: source.clone(),
        workspace_name: "alder".into(),
        branch: branch.into(),
        base_ref: None,
    };

    let named = ask(NamesGitHas { repo_name: "repo".into(), source: source.clone() }.into_request()).await;
    let Ok(WorkerResponse::WorkspaceReady(ready)) = ask(making.into_request()).await else { panic!("not made") };
    let (path, worktree) = (ready.path.clone(), Path::new(&ready.path));
    run(worktree, &["commit", "-q", "--allow-empty", "-m", "work"]);
    let work = run(worktree, &["rev-parse", "HEAD"]);
    let set_up = ask(SetupWorkspace { source: source.clone(), path: path.clone(), env: vec![] }.into_request()).await;
    ask(ArchiveScript { source: source.clone(), path: path.clone(), env: vec![] }.into_request()).await.unwrap();
    let removing = RemoveWorkspace { source: source.clone(), path: path.clone(), force: false, own: None };
    ask(removing.into_request()).await.unwrap();
    let archived = !worktree.exists();
    ask(RestoreWorkspace { source, path, branch: branch.into(), archived: None }.into_request()).await.unwrap();

    let git_has = ["HEAD", "main", "origin"].map(String::from).to_vec();
    assert_eq!(named.unwrap(), WorkerResponse::NamesGitHas(NamesGitHasReply { names: git_has }));
    assert_eq!(set_up.unwrap(), WorkerResponse::SetupDone(SetupDone { code: Some(0) }));
    assert!(archived, "its worktree was removed");
    assert_eq!(run(worktree, &["rev-parse", "HEAD"]), work, "back on its branch, with its work");
    let clones = std::fs::read_dir(dir.path().join("clones")).unwrap().map(|entry| entry.unwrap().path());
    let common = std::fs::canonicalize(run(worktree, &["rev-parse", "--path-format=absolute", "--git-common-dir"]));
    let clones: Vec<_> = clones.filter(|clone| !clone.ends_with(".making")).collect();
    assert_eq!(clones.len(), 1, "{clones:?}");
    assert_eq!(std::fs::canonicalize(&clones[0]).unwrap(), common.unwrap(), "made from the clone");
}

#[tokio::test]
async fn capture_archive_and_restore_requests_wire_through_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    run(dir.path(), &["clone", "-q", "--bare", new_repo(dir.path()).to_str().unwrap(), remote.to_str().unwrap()]);
    let worker = Worker::new(config(dir.path()));
    let ask = |request| handle(&worker.state, &worker.forwarders, Ok(request));
    let (source, branch) = (RepoSource::Clone { url: remote.to_string_lossy().into_owned() }, "alder/start");
    let making = PrepareWorkspace {
        repo_name: "repo".into(),
        source: source.clone(),
        workspace_name: "alder".into(),
        branch: branch.into(),
        base_ref: None,
    };
    let Ok(WorkerResponse::WorkspaceReady(ready)) = ask(making.into_request()).await else { panic!("not made") };
    let (path, worktree) = (ready.path.clone(), Path::new(&ready.path));
    run(worktree, &["commit", "-q", "--allow-empty", "-m", "ephemeral work"]);
    let work = run(worktree, &["rev-parse", "HEAD"]);
    let Ok(WorkerResponse::ArchivedGit(archived)) =
        ask(CaptureArchive { source: source.clone(), force: false }.into_request()).await
    else {
        panic!("expected ArchivedGit");
    };
    ask(RemoveWorkspace { source: source.clone(), path: path.clone(), force: false, own: None }.into_request())
        .await
        .unwrap();
    std::fs::remove_dir_all(dir.path().join("clones")).unwrap();
    ask(RestoreWorkspace { source, path, branch: branch.into(), archived: Some(archived) }.into_request())
        .await
        .unwrap();
    assert_eq!(run(worktree, &["rev-parse", "HEAD"]), work);
}
