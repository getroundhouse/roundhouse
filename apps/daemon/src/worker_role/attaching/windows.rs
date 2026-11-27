//! On Windows no worker listens, since there's no Unix socket for it to listen on that tokio has, and the local
//! provider's workers run on their hub's pipes, so only someone running `worker --attach` by hand reaches this.

use crate::worker_role::Given;

pub(crate) fn attach(_given: &Given) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("a worker listens only where there are Unix sockets"))
}
