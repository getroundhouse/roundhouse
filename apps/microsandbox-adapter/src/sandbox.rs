//! Running things in a sandbox, and the port forwarded into it.

use std::io;
use std::time::Duration;

use microsandbox::Sandbox;
use microsandbox::sandbox::ExecOutput;
use providers::ProviderError;

use crate::guest;
use crate::provider::failed;

/// Runs `script` as root, within `within` if it's given, and says what it printed, or, as `what` failing, what it
/// wrote to stderr last.
pub(crate) async fn run(
    sandbox: &Sandbox,
    what: &str,
    script: &str,
    within: Option<Duration>,
) -> Result<String, ProviderError> {
    let output = sandbox
        .shell_with(script, |exec| match within {
            Some(within) => exec.timeout(within),
            None => exec,
        })
        .await
        .map_err(|e| failed(what, &e))?;
    said(what, &output)
}

/// Runs the worker's binary with `args`, as the worker's user, with what a login gives it, if `as_its_user`, and as
/// root otherwise.
pub(crate) async fn run_worker(
    sandbox: &Sandbox,
    what: &str,
    args: Vec<String>,
    as_its_user: bool,
) -> Result<String, ProviderError> {
    let output = sandbox
        .exec_with(guest::WORKER, |exec| {
            let exec = exec.args(args);
            if as_its_user { exec.user(guest::USER).envs(guest::worker_env()) } else { exec }
        })
        .await
        .map_err(|e| failed(what, &e))?;
    said(what, &output)
}

fn said(what: &str, output: &ExecOutput) -> Result<String, ProviderError> {
    if output.status().success {
        return Ok(String::from_utf8_lossy(output.stdout_bytes()).into_owned());
    }
    let stderr = String::from_utf8_lossy(output.stderr_bytes());
    let lines: Vec<&str> = stderr.trim().lines().collect();
    let last = lines[lines.len().saturating_sub(5)..].join("\n");
    Err(ProviderError::io(what, io::Error::other(format!("it exited {}: {last}", output.status().code))))
}

/// A UDP port on the hub's machine's loopback that nothing holds now, to forward into a sandbox. Something else could
/// take it before the sandbox does, which fails making the sandbox, and a make that's asked again picks another.
pub(crate) fn free_udp_port() -> Result<u16, ProviderError> {
    let socket = std::net::UdpSocket::bind(("127.0.0.1", 0))
        .map_err(|e| ProviderError::io("finding a free UDP port to forward", e))?;
    socket.local_addr().map(|addr| addr.port()).map_err(|e| ProviderError::io("finding a free UDP port to forward", e))
}

/// The port on the hub's machine forwarded to the worker's in the sandbox, as it was made with.
pub(crate) fn forwarded_port(sandbox: &Sandbox) -> Result<u16, ProviderError> {
    let ports = &sandbox.config().spec.network.ports;
    let forwarded = ports.iter().find(|port| {
        port.guest_port == guest::WORKER_PORT && port.protocol == microsandbox::sandbox::PortProtocol::Udp
    });
    forwarded.map(|port| port.host_port).ok_or_else(|| {
        let why = format!(
            "no UDP port is forwarded to the worker's, {}, so it was made by an older adapter",
            guest::WORKER_PORT
        );
        ProviderError::io("starting the worker", io::Error::other(why))
    })
}
