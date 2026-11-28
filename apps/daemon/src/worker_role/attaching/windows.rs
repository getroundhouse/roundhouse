//! On Windows no worker listens, since there's no Unix socket for it to listen on that tokio has, and an earlier build's
//! local workers there ran on their hub's pipes and stopped with it, so none is left for this to reach. A hub runs it
//! only for a worker it finds running on an instance with no record of its dialing, which it then ends, as it ends any
//! worker it can't reach.

use crate::worker_role::Given;

pub(crate) fn attach(_given: &Given) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("a worker listens only where there are Unix sockets"))
}
