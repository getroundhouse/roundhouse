//! On Windows a worker can't outlive its hub, since it would listen on a Unix socket, which tokio has nowhere else. The
//! local provider has its workers run on their hub's pipes there instead, as "Providers" in `docs/architecture.md`
//! says, so only someone running `worker --attach` or `--listen` by hand reaches these.

use crate::worker_role::Given;

pub(crate) fn attach(_given: &Given) -> anyhow::Result<()> {
    Err(cannot())
}

pub(crate) fn listen(_given: Given) -> anyhow::Result<()> {
    Err(cannot())
}

fn cannot() -> anyhow::Error {
    anyhow::anyhow!("a worker can outlive its hub only where there are Unix sockets")
}
