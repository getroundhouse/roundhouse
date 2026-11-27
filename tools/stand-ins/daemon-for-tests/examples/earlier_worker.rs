//! Stands in for a worker an earlier build left listening on a socket in its state directory, as that build's local
//! workers did to outlive their hub, which this build's `worker --attach` reaches, and no longer starts. It runs this
//! build's worker on its stdin and stdout, named by the first argument, with the rest of its arguments, and relays to
//! it the first connection to its socket, which is all a hub reaching it through `--attach` meets of such a worker,
//! until the hub ends it. Run as `earlier_worker <daemon> --workspaces-root … --state-dir … --clones-dir …`.

#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    unix::main()
}

/// No worker listens where there are no Unix sockets.
#[cfg(not(unix))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("a worker listens only where there are Unix sockets")
}

#[cfg(unix)]
mod unix {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::UnixListener;
    use std::os::unix::process::CommandExt as _;
    use std::path::PathBuf;
    use std::process::{ChildStdin, ChildStdout, Command, Stdio};

    use anyhow::Context;

    pub(super) fn main() -> anyhow::Result<()> {
        let mut args = std::env::args_os().skip(1);
        let daemon = PathBuf::from(args.next().context("the daemon to run the worker with")?);
        let given: Vec<_> = args.collect();
        let state_dir = given
            .iter()
            .position(|arg| arg == "--state-dir")
            .and_then(|at| given.get(at + 1))
            .map(PathBuf::from)
            .context("--state-dir")?;
        std::fs::create_dir_all(&state_dir)?;
        // So the socket is named relative to it, as an earlier build's was, since the whole path can be longer than a
        // socket's may be.
        std::env::set_current_dir(&state_dir)?;
        // Leading a group of its own, as an earlier build's did, which is how its provider ends it.
        let mut worker = Command::new(daemon)
            .arg("worker")
            .arg("--stdio")
            .args(&given)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (to_worker, from_worker) = worker.stdin.take().zip(worker.stdout.take()).context("the worker's pipes")?;
        let listener = UnixListener::bind(protocol::paths::WORKER_SOCKET_FILE)?;
        std::thread::spawn(move || serve(&listener, to_worker, from_worker));
        // Once the worker has gone, so has this, as with an earlier build's worker, whatever it was serving.
        worker.wait()?;
        let _ = std::fs::remove_file(protocol::paths::WORKER_SOCKET_FILE);
        Ok(())
    }

    /// Relays the first hub that connects to the worker, until either goes.
    fn serve(listener: &UnixListener, mut to_worker: ChildStdin, mut from_worker: ChildStdout) -> std::io::Result<()> {
        let (hub, _) = listener.accept()?;
        let mut from_hub = hub.try_clone()?;
        // Dropping the worker's stdin once the hub has gone closes it, as a hub that goes does.
        std::thread::spawn(move || relay(&mut from_hub, &mut to_worker));
        relay(&mut from_worker, &mut &hub)?;
        hub.shutdown(Shutdown::Write)
    }

    /// Copies `from` to `to`, each read as it comes, until `from` ends.
    fn relay(from: &mut impl Read, to: &mut impl Write) -> std::io::Result<()> {
        let mut buffer = vec![0; 64 * 1024];
        loop {
            match from.read(&mut buffer)? {
                0 => return Ok(()),
                read => {
                    to.write_all(&buffer[..read])?;
                    to.flush()?;
                }
            }
        }
    }
}
