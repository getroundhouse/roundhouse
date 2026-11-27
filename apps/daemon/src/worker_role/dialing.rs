//! A worker that dials its hub with a ticket, as `protocol::dialing` says, and dials again whenever the connection
//! drops, so it outlives a hub that restarts. A provider whose machines sit behind a port mapping, such as a VM with a
//! forwarded port, has it bind the port and advertise the mapped address, so the hub opens a direct path to it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use dialing::{Admitted, DialError, WorkerEndpoint};
use protocol::dialing::WorkerTicket;
use tracing::info;
use worker::DialFailed;

use crate::runtime::run_to_end;
use crate::worker_role::{Given, config};

/// How long a worker that dials goes on without reaching its hub before it stops its agents, so nothing runs for good
/// with no hub to record it, as `docs/principles/failure.md` says. Longer than a hub takes to restart, a migration
/// included, as for a worker that listens.
const WITHOUT_A_HUB: Duration = Duration::from_mins(10);

/// How long one dial has, after which it's tried again, so a hub that's gone doesn't hold the worker in one attempt.
const DIAL_WITHIN: Duration = Duration::from_secs(10);

/// How the worker dials its hub.
pub(crate) struct Dial {
    pub(crate) ticket: WorkerTicket,
    /// The UDP port to bind, such as one forwarded into the VM it runs in. Any, if `None`.
    pub(crate) port: Option<u16>,
    /// Where the hub can reach it directly, such as the host's end of that forwarded port.
    pub(crate) advertise: Option<SocketAddr>,
    /// Whether to start it in a session of its own, and return once it has started.
    pub(crate) detach: bool,
}

/// What `--ticket` names in place of a ticket for the worker to read it from its stdin, as the local provider gives it,
/// since any user on the machine can read a process's arguments.
pub(crate) const TICKET_ON_STDIN: &str = "-";

/// In the worker's state directory, which it claims first, so a second worker started there by mistake leaves the
/// first to serve it.
pub(crate) fn dial(mut given: Given, Dial { ticket, port, advertise, detach }: Dial) -> anyhow::Result<()> {
    let on_stdin = ticket.as_str() == TICKET_ON_STDIN;
    let ticket = if on_stdin {
        let mut read = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut read).context("reading the ticket from stdin")?;
        WorkerTicket::from(read)
    } else {
        ticket
    };
    let read = ticket.read().context("reading the ticket")?;
    if detach {
        return start_detached(on_stdin.then_some(&ticket));
    }
    let ticket = read;
    std::fs::create_dir_all(&given.state_dir).with_context(|| format!("creating {}", given.state_dir.display()))?;
    let Some(_claim) = worker::claim_to_serve(&given.state_dir)? else {
        info!(name: "worker.instance_taken", "Another worker has the instance");
        return Ok(());
    };
    let stops_with = std::mem::replace(&mut given.config.stops_with, Box::pin(std::future::pending()));
    let config = config(given, None)?;
    run_to_end(async move {
        let endpoint = Arc::new(WorkerEndpoint::bind(&ticket.hub, port, advertise).await?);
        info!(name: "hub.dialing", key = endpoint.key(), port = ?port, advertise = ?advertise, "Dialing the hub");
        let dial = || {
            let (endpoint, secret) = (endpoint.clone(), ticket.secret.clone());
            async move {
                let admitted = tokio::time::timeout(DIAL_WITHIN, endpoint.dial_admitted(&secret)).await;
                match admitted {
                    Ok(Ok(Admitted { connection, send, recv, instance_id })) => {
                        info!(name: "hub.admitted", %instance_id, "The hub admitted this worker");
                        // The connection closes once it's dropped, so it goes with the stream the hub writes on.
                        Ok((recv, Holding { send, _connection: connection }))
                    }
                    Ok(Err(DialError::Refused(why))) => Err(DialFailed::Refused(why)),
                    Ok(Err(e)) => Err(DialFailed::Unreachable(e.to_string())),
                    Err(_) => Err(DialFailed::Unreachable(format!("no answer within {DIAL_WITHIN:?}"))),
                }
            }
        };
        let served = worker::serve_dialing(dial, config, WITHOUT_A_HUB, stops_with).await;
        endpoint.close().await;
        served.map_err(anyhow::Error::from)
    })?
}

/// This command again, without `--detach`, in a session of its own, so nothing that ends what ran this reaches it. It
/// writes its log to the file `--log` names, and nothing to what this was given. `on_stdin`, the ticket this read from
/// its stdin, goes on the worker's, which the same arguments have it read from.
fn start_detached(on_stdin: Option<&WorkerTicket>) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("finding this binary to start the worker with")?;
    let mut command = std::process::Command::new(exe);
    command
        .args(std::env::args_os().skip(1).filter(|arg| arg != "--detach"))
        .stdin(if on_stdin.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    process::detach(&mut command);
    let mut started = command.spawn().context("starting the worker")?;
    if let (Some(mut stdin), Some(ticket)) = (started.stdin.take(), on_stdin) {
        std::io::Write::write_all(&mut stdin, ticket.as_str().as_bytes()).context("giving the worker its ticket")?;
    }
    info!(name: "worker.detached", pid = started.id(), "Started the worker in a session of its own");
    Ok(())
}

/// The stream a worker writes to its hub on, with the connection it's on, which closes once both are dropped.
struct Holding {
    send: dialing::SendStream,
    _connection: dialing::Connection,
}

impl tokio::io::AsyncWrite for Holding {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.send).poll_write(cx, buf).map_err(std::io::Error::from)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.send).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.send).poll_shutdown(cx)
    }
}
