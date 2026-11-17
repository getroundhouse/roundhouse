//! What a hub tells a worker to make a workspace's worktrees from, in each request that names it: a checkout, as every
//! worker reads it, or an address to clone, which a worker from before refuses. The checkout's form, beside the rest
//! of each request, is spelled out with the rest in `worker_calls.rs`.

use protocol::frame;
use protocol::model::BranchName;
use protocol::worker::{
    AddChildWorktree, ArchiveScript, ChildBranch, NamesGitHas, PrepareWorkspace, RemoveWorkspace, RepoSource,
    Request as _, RestoreWorkspace, SetupWorkspace, WorkerRequest,
};
use serde::Deserialize;

use super::worker_calls::asked;

const URL: &str = "https://github.com/ada/rails.git";

/// Every request that names a source, made from `source`, each with what the rest of its form is, after its method.
fn every_request(source: &RepoSource) -> Vec<(WorkerRequest, &'static str, &'static str)> {
    let source = || source.clone();
    let branch = BranchName::from("birch");
    let on = ChildBranch::Existing { branch: branch.clone() };
    vec![
        (
            NamesGitHas { repo_name: "r".into(), source: source() }.into_request(),
            "names_git_has",
            r#""repo_name":"r","#,
        ),
        (
            PrepareWorkspace {
                repo_name: "r".into(),
                source: source(),
                workspace_name: "birch".into(),
                branch: branch.clone(),
                base_ref: None,
            }
            .into_request(),
            "prepare_workspace",
            r#""repo_name":"r","#,
        ),
        (RestoreWorkspace { source: source(), path: "/w".into(), branch }.into_request(), "restore_workspace", ""),
        (
            RemoveWorkspace { source: source(), path: "/w".into(), force: false, own: None }.into_request(),
            "remove_workspace",
            "",
        ),
        (
            AddChildWorktree { source: source(), own: "/w".into(), path: "/c/w/x".into(), on }.into_request(),
            "add_child_worktree",
            "",
        ),
        (SetupWorkspace { source: source(), path: "/w".into(), env: vec![] }.into_request(), "setup_workspace", ""),
        (ArchiveScript { source: source(), path: "/w".into(), env: vec![] }.into_request(), "archive_script", ""),
    ]
}

/// What follows each request's source, which is the same whatever its source is.
fn after_source(method: &str) -> &'static str {
    match method {
        "names_git_has" => "",
        "prepare_workspace" => r#","workspace_name":"birch","branch":"birch","base_ref":null"#,
        "restore_workspace" => r#","path":"/w","branch":"birch""#,
        "remove_workspace" => r#","path":"/w","force":false"#,
        "add_child_worktree" => r#","own":"/w","path":"/c/w/x","on":{"kind":"existing","branch":"birch"}"#,
        _ => r#","path":"/w","env":[]"#,
    }
}

/// A clone is written as its address alone, without the `repo_path` that a worker from before reads, and read back as
/// it was written.
#[test]
fn every_request_names_a_clone_by_its_address_alone() {
    let clone = RepoSource::Clone { url: URL.into() };

    for (request, method, before) in every_request(&clone) {
        let form = format!(
            r#"{{"method":"{method}",{before}"source":{{"kind":"clone","url":"{URL}"}}{}}}"#,
            after_source(method)
        );
        asked(request, &form);
    }
}

/// A hub from before sources, as after a rollback, names a checkout as `repo_path` alone, which reads as one.
#[test]
fn a_checkout_named_as_a_hub_from_before_sources_names_it_reads_as_one() {
    let checkout = RepoSource::Checkout { path: "/r".into() };

    for (request, method, before) in every_request(&checkout) {
        let form =
            format!(r#"{{"id":7,"request":{{"method":"{method}",{before}"repo_path":"/r"{}}}}}"#, after_source(method));
        let call = frame::read_call::<WorkerRequest>(form.as_bytes()).unwrap();
        assert_eq!(call.request.unwrap(), request, "{form}");
    }
}

/// What a worker from before sources reads of a request, which is every field it has, and a `repo_path` it needs.
#[derive(Deserialize)]
struct BeforeSources {
    repo_path: String,
}

/// A worker from before sources reads a checkout's `repo_path` as it always has, and can't read a request to clone at
/// all, so it refuses it, rather than make a worktree of a checkout that isn't there.
#[test]
fn a_worker_from_before_sources_reads_a_checkout_and_refuses_a_clone() {
    let written = |source: &RepoSource| {
        every_request(source).into_iter().map(|(request, ..)| serde_json::to_string(&request).unwrap()).collect()
    };
    let checkouts: Vec<String> = written(&RepoSource::Checkout { path: "/r".into() });
    let clones: Vec<String> = written(&RepoSource::Clone { url: URL.into() });

    for form in checkouts {
        assert_eq!(serde_json::from_str::<BeforeSources>(&form).unwrap().repo_path, "/r", "{form}");
    }
    for form in clones {
        assert!(serde_json::from_str::<BeforeSources>(&form).is_err(), "{form}");
    }
}

/// A source of a kind this build doesn't know makes the request unreadable, so the worker refuses it, even with a
/// `repo_path` beside it, rather than make the worktree from that checkout. So does a request that names no source.
#[test]
fn a_source_of_a_kind_this_build_doesnt_know_or_none_at_all_isnt_read() {
    let newer = br#"{"id":7,"request":{"method":"prepare_workspace","repo_name":"r","repo_path":"/r","source":{"kind":"mirror","url":"x"},"workspace_name":"b","branch":"b","base_ref":null}}"#;
    let none = br#"{"id":8,"request":{"method":"restore_workspace","path":"/w","branch":"b"}}"#;

    for (line, id) in [(&newer[..], 7), (&none[..], 8)] {
        let call = frame::read_call::<WorkerRequest>(line).unwrap();
        assert!((call.id, call.request.is_err()) == (id, true), "{call:?}");
    }
}
