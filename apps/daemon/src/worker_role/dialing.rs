//! A worker that dials its hub with a ticket, as `protocol::dialing` says, and dials again whenever the connection
//! drops, so it outlives a hub that restarts. A provider whose machines sit behind a port mapping, such as a VM with a
//! forwarded port, has it bind the port and advertise the mapped address, so the hub opens a direct path to it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use dialing::{Admitted, DialError, WorkerEndpoint};
use protocol::dialing::WorkerTicket;
use tracing::{debug, info, warn};
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
    /// Where the hub says its loopback endpoint is now, as `dialing::address_file` says, for a local worker.
    pub(crate) hub_address_file: Option<PathBuf>,
    /// Whether to start it apart from whatever ran this, and return once it has started.
    pub(crate) detach: bool,
}

/// What `--ticket` names in place of a ticket for the worker to read it from its stdin, as the local provider gives it,
/// since any user on the machine can read a process's arguments.
pub(crate) const TICKET_ON_STDIN: &str = "-";

/// In the worker's state directory, which it claims first, so a second worker started there by mistake leaves the
/// first to serve it.
pub(crate) fn dial(
    mut given: Given,
    Dial { ticket, port, advertise, hub_address_file, detach }: Dial,
) -> anyhow::Result<()> {
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
        let looking_up = Arc::new(LookingUp { file: hub_address_file, told_of_another: AtomicBool::new(false) });
        let dial = || {
            let (endpoint, secret, looking_up) = (endpoint.clone(), ticket.secret.clone(), looking_up.clone());
            async move {
                let dial_once = || tokio::time::timeout(DIAL_WITHIN, endpoint.dial_admitted(&secret));
                let mut admitted = dial_once().await;
                // Where the hub is now, at once, rather than after the pause before the next dial.
                if !matches!(admitted, Ok(Ok(_) | Err(DialError::Refused(_)))) && looking_up.again(&endpoint) {
                    admitted = dial_once().await;
                }
                match admitted {
                    Ok(Ok(Admitted { connection, send, recv, instance_id })) => {
                        info!(name: "hub.admitted", %instance_id, "The hub admitted this worker");
                        // The connection closes once it's dropped, so it goes with the stream the hub writes on.
                        Ok((recv, Holding::new(send, connection)))
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

/// Where to look the hub up again once a dial has failed, as `dialing::address_file` says.
struct LookingUp {
    file: Option<PathBuf>,
    /// Whether the worker has said the file names another hub than its ticket, which it says once.
    told_of_another: AtomicBool,
}

impl LookingUp {
    /// Has `endpoint` dial the hub where the file says it is now, if it says somewhere else, and it's the ticket's hub,
    /// and says whether it does.
    fn again(&self, endpoint: &WorkerEndpoint) -> bool {
        let Some(file) = &self.file else { return false };
        let address = match dialing::address_file::read(file) {
            Ok(address) => address,
            Err(e) => {
                debug!(name: "hub.address_unread", error = %e, "Couldn't read where the hub is now");
                return false;
            }
        };
        match endpoint.moved_to(&address) {
            Ok(moved) => {
                if moved {
                    info!(name: "hub.moved", direct = ?address.direct, "The hub is somewhere else now, where it's dialed from now on");
                }
                moved
            }
            Err(e) => {
                if !self.told_of_another.swap(true, Ordering::Relaxed) {
                    warn!(name: "hub.address_of_another", error = %e, "The hub's address file names another hub, so it isn't dialed there");
                }
                false
            }
        }
    }
}

/// This command again, without `--detach`, apart from this process, as `process::detach` starts it, so nothing that
/// ends what ran this reaches it: on Unix in a session of its own, and on Windows with a console of its own, out of the
/// job object whatever ran this is in, where the job lets it. It writes its log to the file `--log` names, and nothing
/// to what this was given. `on_stdin`, the ticket this read from its stdin, goes on the worker's, which the same
/// arguments have it read from.
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
    info!(name: "worker.detached", pid = started.id(), "Started the worker apart from what ran this");
    Ok(())
}

/// The stream a worker writes to its hub on, with the connection it's on, which closes once both are dropped. Closing a
/// connection drops whatever the hub hasn't yet acknowledged, such as what agents said as they stopped, so shutting
/// this finishes the stream and then waits, up to [`HUB_HAS_IT_WITHIN`], until the hub has acknowledged all of it.
struct Holding {
    send: dialing::SendStream,
    _connection: dialing::Connection,
    acknowledged: Acknowledged,
}

/// How far shutting a [`Holding`] has got.
enum Acknowledged {
    /// Not started, so the stream is still to be finished.
    Finishing,
    /// Finished, and waiting for the hub to acknowledge it all.
    Waiting(std::pin::Pin<Box<dyn Future<Output = ()> + Send>>),
    Done,
}

/// How long a worker that's done waits for its hub to acknowledge the last of what it sent, which a hub that's there
/// does within milliseconds, and one that's gone never does.
const HUB_HAS_IT_WITHIN: Duration = Duration::from_secs(5);

impl Holding {
    fn new(send: dialing::SendStream, connection: dialing::Connection) -> Self {
        Self { send, _connection: connection, acknowledged: Acknowledged::Finishing }
    }
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
        if matches!(self.acknowledged, Acknowledged::Finishing) {
            // Finishing a stream that's finished already, or that the hub stopped, leaves nothing to wait for.
            if self.send.finish().is_err() {
                self.acknowledged = Acknowledged::Done;
                return std::task::Poll::Ready(Ok(()));
            }
            let stopped = self.send.stopped();
            self.acknowledged = Acknowledged::Waiting(Box::pin(async move {
                let _ = tokio::time::timeout(HUB_HAS_IT_WITHIN, stopped).await;
            }));
        }
        if let Acknowledged::Waiting(acknowledged) = &mut self.acknowledged {
            std::task::ready!(acknowledged.as_mut().poll(cx));
            self.acknowledged = Acknowledged::Done;
        }
        std::task::Poll::Ready(Ok(()))
    }
}
