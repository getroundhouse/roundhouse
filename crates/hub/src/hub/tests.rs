//! What the hub's own tests share.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::model::{Checks, Local, MergeState, PrState, PullRequest, Review};
use protocol::worker::{WorkerRequest, WorkerResponse};
use providers::local::{LocalConfig, LocalProvider};

use super::Hub;
use crate::provider_set::ProviderSet;
use crate::store::tests::store_with_session;
use crate::worker_link::WorkerLink;

/// A hub whose store has the workspace `w` and its session `s`, with nothing sent in it, and that never starts a
/// worker.
pub(super) fn hub_in(dir: &Path) -> Arc<Hub> {
    let store = Arc::new(store_with_session(dir));
    let config = LocalConfig {
        worker_exe: dir.join("no-worker"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        shell: Some("/bin/sh".to_string()),
        path: None,
    };
    let providers = ProviderSet::only(Arc::new(LocalProvider::new(config)));
    Arc::new(Hub::new(store, dir.join("attachments"), dir.join("worker-logs"), providers, false))
}

/// The same, with `w`'s worktree at `/w`, so its pull request can be read through a stand-in for its worker.
pub(super) fn hub_with_worktree(dir: &Path) -> Arc<Hub> {
    let hub = hub_in(dir);
    hub.store.workspace_ready(&"w".into(), "/w", "alder", "main").unwrap();
    hub
}

/// An open pull request whose checks are running.
pub(super) fn open() -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/7".into(),
        title: "T".into(),
        state: PrState::Open,
        is_draft: false,
        base: None,
        merge_state: MergeState::Blocked,
        review: Review::NotRequired,
        checks: Checks { passed: 0, failed: 0, pending: 1 },
        queue: None,
        local: Local::default(),
    }
}

/// What a stand-in worker says of the pull request when it's read, which a test changes as it goes.
pub(super) type Says = Arc<Mutex<Result<Option<PullRequest>, ApiError>>>;

pub(super) fn says(pull_request: Option<PullRequest>) -> Says {
    Arc::new(Mutex::new(Ok(pull_request)))
}

/// A stand-in for `w`'s worker, as the hub's, that reads its pull request as `says` holds then, and does whatever
/// else it's asked without a word, so it can't say how much of GitHub's limit is left, or how the worktree stands.
pub(super) fn reading(hub: &Hub, says: &Says) -> Arc<WorkerLink> {
    let says = says.clone();
    let link = WorkerLink::answering(move |request| {
        Some(if matches!(request, WorkerRequest::PullRequest { .. }) {
            says.lock().clone().map(|pull_request| WorkerResponse::PullRequest { pull_request, branch: None })
        } else {
            Ok(WorkerResponse::Ok)
        })
    });
    hub.workers.lock().insert("w".into(), link.clone());
    link
}
