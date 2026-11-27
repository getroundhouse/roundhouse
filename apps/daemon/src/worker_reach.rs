//! How a worker reaches its hub, as its flags say.

use crate::worker_role::Transport;

#[derive(clap::Args)]
pub(crate) struct WorkerReach {
    /// Speak the worker protocol on stdin and stdout, so the worker stops when its hub goes. No provider starts one
    /// that way any more, since every worker dials its hub. A test runs one on its pipes, and one behind a socket
    /// stands in for a worker an earlier build left listening, which `--attach` reaches, so it goes with that.
    #[arg(long, group = "transport")]
    stdio: bool,
    /// Connect stdin and stdout to the instance's worker, which an earlier build left listening on a socket in its
    /// state directory. Starts none. Not on Windows.
    #[arg(long, group = "transport")]
    attach: bool,
    /// Dial the hub with the ticket its provider was given, and again whenever the connection drops, so it can
    /// outlive its hub. `-` reads it from stdin, which no other user on the machine can, as they can a process's
    /// arguments.
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
    /// Clap refuses more than one, and none. `detach` is whether a worker that dials starts apart from whatever ran
    /// this, as `process::detach` starts it.
    pub(crate) fn transport(self, detach: bool) -> Transport {
        match self {
            Self { attach: true, .. } => Transport::Attach,
            Self { ticket: Some(ticket), port, advertise, .. } => {
                Transport::Dial(crate::worker_role::Dial { ticket: ticket.into(), port, advertise, detach })
            }
            Self { .. } => Transport::Stdio,
        }
    }
}
