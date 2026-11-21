//! How a worker reaches its hub, as its flags say.

use crate::worker_role::Transport;

#[derive(clap::Args)]
pub(crate) struct WorkerReach {
    /// Speak the worker protocol on stdin and stdout, so the worker stops when its hub goes.
    #[arg(long, group = "transport")]
    stdio: bool,
    /// Connect stdin and stdout to the instance's worker, starting one if none is serving it, so it can outlive
    /// its hub. Not on Windows.
    #[arg(long, group = "transport")]
    attach: bool,
    /// Serve the instance on a socket in the worker's state directory, as `--attach` starts it.
    #[arg(long, group = "transport", hide = true)]
    listen: bool,
    /// Dial the hub with the ticket its provider was given, and again whenever the connection drops, so it can
    /// outlive its hub. Not on Windows.
    #[arg(long, group = "transport")]
    ticket: Option<String>,
    /// The UDP port to dial from, such as one forwarded into the machine the worker runs on. Any, without it.
    #[arg(long, requires = "ticket")]
    port: Option<u16>,
    /// An address the hub can reach the worker at directly, such as the far end of that forwarded port.
    #[arg(long, requires = "ticket")]
    advertise: Option<std::net::SocketAddr>,
}

impl WorkerReach {
    /// Whether it's the command a hub runs to reach a worker that outlives it, which runs nothing in a workspace.
    pub(crate) fn attaches(&self) -> bool {
        self.attach
    }

    /// Clap refuses more than one, and none. `detach` is whether a worker that dials starts in a session of its own. A worker dials its hub only where there are Unix sockets, as for the other
    /// workers that outlive their hub, since what claims an instance is there only.
    #[cfg_attr(unix, expect(clippy::unnecessary_wraps, reason = "only Windows refuses one"))]
    #[cfg_attr(
        not(unix),
        expect(unused_variables, reason = "only a worker that dials detaches, and that's only on Unix")
    )]
    pub(crate) fn transport(self, detach: bool) -> anyhow::Result<Transport> {
        Ok(match self {
            Self { attach: true, .. } => Transport::Attach,
            Self { listen: true, .. } => Transport::Listen,
            #[cfg(unix)]
            Self { ticket: Some(ticket), port, advertise, .. } => {
                Transport::Dial(crate::worker_role::Dial { ticket: ticket.into(), port, advertise, detach })
            }
            #[cfg(not(unix))]
            Self { ticket: Some(_), .. } => anyhow::bail!("a worker dials its hub only on Unix"),
            Self { .. } => Transport::Stdio,
        })
    }
}
