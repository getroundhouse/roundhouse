//! What the claim a worker holds on its instance says of whether the worker is running.

use crate::WorkerLiveness;
use protocol::paths::WORKER_LOCK_FILE;
use std::path::Path;

/// Whether a worker holds the claim on the instance whose state directory is `dir`, which `worker::claim_instance` takes
/// and the system lets go of when its holder dies, however it dies. So it's the one thing that says a worker has gone
/// rather than that nobody has seen it.
///
/// It asks for the lock shared, to see whether the worker's exclusive one is held, and lets go at once. It never
/// creates the file, since a worker that is starting would then find it already there, and never waits for the lock.
/// A worker that starts as it looks fails to take the claim for as long as the look lasts, which is moments, and
/// `worker::claim_instance` looks again for longer than that before it takes the claim to be another worker's. So the
/// look never gets in the way of a worker that's taking the claim, nor of one that holds it, which the shared lock
/// never disturbs.
///
/// Only a claim nobody holds is [`WorkerLiveness::Gone`]. A directory with no lock file, one that can't be opened, or a
/// lock that can't be asked for is `Unknown`: the claim is held once a worker starts, so nothing says that none ever
/// will, and a hub that took that for proof would end a worker that's about to serve the instance.
pub(super) fn claim_liveness(dir: &Path) -> WorkerLiveness {
    let Ok(lock) = std::fs::File::open(dir.join(WORKER_LOCK_FILE)) else {
        return WorkerLiveness::Unknown;
    };
    match lock.try_lock_shared() {
        Ok(()) => WorkerLiveness::Gone,
        Err(std::fs::TryLockError::WouldBlock) => WorkerLiveness::Running,
        Err(std::fs::TryLockError::Error(_)) => WorkerLiveness::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Provider as _;
    use crate::local::{LocalConfig, LocalProvider};
    use protocol::model::InstanceId;
    use std::time::Duration;

    fn provider(dir: &Path) -> LocalProvider {
        LocalProvider::new(LocalConfig {
            worker_exe: "/bin/roundhoused".into(),
            instances_dir: dir.join("instances"),
            workspaces_root: dir.join("workspaces"),
            clones_dir: dir.join("clones"),
            worker_logs_dir: dir.join("worker-logs"),
            environment: process::Environment::NONE,
            dials: false,
            hub_address_file: None,
        })
    }

    /// Takes the claim a worker takes on the instance in `dir`, as the worker does, which is held until it's dropped.
    fn hold_claim(dir: &Path) -> std::fs::File {
        std::fs::create_dir_all(dir).unwrap();
        let claim = std::fs::File::create(dir.join(WORKER_LOCK_FILE)).unwrap();
        claim.try_lock().unwrap();
        claim
    }

    #[tokio::test]
    async fn a_worker_holding_its_claim_is_running_and_one_that_let_go_of_it_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let provider = provider(dir.path());
        let id: InstanceId = "a".into();
        provider.create(&id, None, None).await.unwrap();
        let claim = hold_claim(&dir.path().join("instances/a"));

        let held = provider.worker_state(&id).await.unwrap();
        // As the system lets go of it when the worker dies. A process another test is starting holds a copy of every open
        // file until it has started, so a claim let go a moment ago can still read as held, which is no harm to the hub,
        // since it waits for the worker then.
        drop(claim);
        let instance = dir.path().join("instances/a");
        waiting::task::until(Duration::from_secs(5), "the claim to be let go", || {
            claim_liveness(&instance) == WorkerLiveness::Gone
        })
        .await;
        let let_go = provider.worker_state(&id).await.unwrap();

        assert_eq!((held, let_go), (WorkerLiveness::Running, WorkerLiveness::Gone));
    }

    /// Only proof is gone: an instance no worker has claimed yet is one a worker may be about to, and asking leaves it so.
    #[tokio::test]
    async fn an_instance_nobody_has_claimed_is_unknown_and_stays_unclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let provider = provider(dir.path());
        let id: InstanceId = "a".into();
        provider.create(&id, None, None).await.unwrap();
        let missing: InstanceId = "b".into();

        let unclaimed = provider.worker_state(&id).await.unwrap();
        let not_there = provider.worker_state(&missing).await.unwrap();

        assert_eq!((unclaimed, not_there), (WorkerLiveness::Unknown, WorkerLiveness::Unknown));
        assert!(!dir.path().join("instances/a").join(WORKER_LOCK_FILE).exists(), "asking made the claim's file");
    }
}
