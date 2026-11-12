//! The runtime a hub or a worker runs on, and how it lets go once that role has stopped.

use std::time::Duration;

/// How long a hub or a worker waits, once it has stopped, for what's left on its runtime's blocking threads. Nothing
/// is waiting for what's there by then, such as a write to the database for a request the stop cut off, which takes
/// milliseconds. On Windows, though, tokio reads a child's output on one of those threads, and the read only ends once
/// everything holding the pipe's other end has exited. That can be something an agent left running, which may never
/// exit, and waiting for it would keep this process running after it had stopped, and with it a hub's pipe and every
/// connection to it.
const EXIT_GRACE: Duration = Duration::from_secs(1);

/// Runs `role` until it finishes, then lets the runtime go within [`EXIT_GRACE`].
pub(crate) fn run_to_end<F: Future>(role: F) -> std::io::Result<F::Output> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let output = runtime.block_on(role);
    runtime.shutdown_timeout(EXIT_GRACE);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_ends_without_waiting_for_a_blocking_thread_that_never_does() {
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            run_to_end(async {
                // As a read of a pipe that something left running holds open.
                drop(tokio::task::spawn_blocking(|| {
                    loop {
                        std::thread::park();
                    }
                }));
            })
            .unwrap();
            done.send(()).unwrap();
        });

        assert!(finished.recv_timeout(Duration::from_secs(10)).is_ok(), "it waited for the blocking thread");
    }
}
