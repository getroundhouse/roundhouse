//! What a hub tells a worker to make a workspace's worktrees from, in each request that names it, as its `source`: a
//! checkout or an address to clone, which a worker of protocol 5 reads too. The checkout's form, beside the rest of
//! each request, is spelled out with the rest in `worker_calls.rs`.

use protocol::frame;
use protocol::model::BranchName;
use protocol::worker::{
    AddChildWorktree, ArchiveScript, CaptureArchive, ChildBranch, NamesGitHas, PrepareWorkspace, RemoveWorkspace,
    RepoSource, Request as _, RestoreWorkspace, SetupWorkspace, WorkerRequest,
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
        (CaptureArchive { source: source(), force: false }.into_request(), "capture_archive", ""),
        (
            RestoreWorkspace { source: source(), path: "/w".into(), branch, archived: None }.into_request(),
            "restore_workspace",
            "",
        ),
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
        "names_git_has" | "capture_archive" => "",
        "prepare_workspace" => r#","workspace_name":"birch","branch":"birch","base_ref":null"#,
        "restore_workspace" => r#","path":"/w","branch":"birch""#,
        "remove_workspace" => r#","path":"/w","force":false"#,
        "add_child_worktree" => r#","own":"/w","path":"/c/w/x","on":{"kind":"existing","branch":"birch"}"#,
        _ => r#","path":"/w","env":[]"#,
    }
}

/// Each source is written as `source` alone, and read back as it was written.
#[test]
fn every_request_names_its_source_alone() {
    let sources = [
        (RepoSource::Checkout { path: "/r".into() }, r#"{"kind":"checkout","path":"/r"}"#.to_string()),
        (RepoSource::Clone { url: URL.into() }, format!(r#"{{"kind":"clone","url":"{URL}"}}"#)),
    ];

    for (source, written) in sources {
        for (request, method, before) in every_request(&source) {
            asked(request, &format!(r#"{{"method":"{method}",{before}"source":{written}{}}}"#, after_source(method)));
        }
    }
}

/// What a worker of protocol 5 reads of a request's source: `source`, or `repo_path` where that's missing, as a hub
/// from before sources sent it.
#[derive(Deserialize)]
struct Protocol5 {
    #[serde(default)]
    repo_path: Option<String>,
    #[serde(default)]
    source: Option<RepoSource>,
}

/// A worker of protocol 5 reads each request's source from `source`, so it needs no `repo_path` beside it.
#[test]
fn a_worker_of_protocol_5_reads_each_requests_source() {
    for source in [RepoSource::Checkout { path: "/r".into() }, RepoSource::Clone { url: URL.into() }] {
        for (request, ..) in every_request(&source) {
            let form = serde_json::to_string(&request).unwrap();

            let Protocol5 { repo_path, source: read } = serde_json::from_str(&form).unwrap();

            assert_eq!((repo_path, read.as_ref()), (None, Some(&source)), "{form}");
        }
    }
}

/// A source of a kind this build doesn't know makes the request unreadable, so the worker refuses it, rather than make
/// the worktree some other way. So does a request that names no source, as a hub from before sources named only its
/// checkout's `repo_path`.
#[test]
fn a_source_of_a_kind_this_build_doesnt_know_or_none_at_all_isnt_read() {
    let newer = br#"{"id":7,"request":{"method":"prepare_workspace","repo_name":"r","source":{"kind":"mirror","url":"x"},"workspace_name":"b","branch":"b","base_ref":null}}"#;
    let none = br#"{"id":8,"request":{"method":"restore_workspace","repo_path":"/r","path":"/w","branch":"b"}}"#;

    for (line, id) in [(&newer[..], 7), (&none[..], 8)] {
        let call = frame::read_call::<WorkerRequest>(line).unwrap();
        assert!((call.id, call.request.is_err()) == (id, true), "{call:?}");
    }
}
