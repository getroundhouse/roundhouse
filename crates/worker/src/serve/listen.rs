//! A worker that can outlive its hub. It serves one hub at a time on a Unix socket, and carries on between them when
//! the hub asked it to.

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::net::UnixListener;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;
use tracing::{error, info, warn};

use super::connection::{self, Closing, Ended};
use super::{Config, Worker};

/// Serves hubs on a socket at `socket`, one at a time, until a hub asks the worker to stop, or a hub goes that didn't
/// ask it to keep running without one, or it has had no hub for `idle_limit`. Then it stops every agent, after sending
/// the last hub, if it's still there, what they said as they stopped. A hub that connects while another is connected is
/// refused, so a hub that asks twice for its connection learns there's one already. The caller holds the instance's
/// claim, so a socket already there is one a worker left behind.
pub async fn listen(socket: &Path, config: Config, idle_limit: Duration) -> io::Result<()> {
    let listener = bind(socket)?;
    let worker = Worker::new(config);
    let mut serving: Option<JoinHandle<Closing>> = None;
    let mut alone_since = Instant::now();
    let last = loop {
        tokio::select! {
            // A hub that has gone first, so the next one, which may have connected since, isn't refused for it.
            biased;
            closing = ending(&mut serving) => {
                serving = None;
                match closing {
                    Ok(closing) if closing.ended == Ended::Stop => break Some(closing),
                    Ok(closing) => {
                        info!("the hub went, and asked this worker to keep running without it");
                        worker.state.outbox.detach().await;
                        closing.abandon();
                    }
                    Err(e) => error!("serving a hub failed: {e}"),
                }
                alone_since = Instant::now();
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let (reader, writer) = stream.into_split();
                    if serving.is_some() {
                        let worker = worker.clone();
                        let refused = Some("another hub is connected to this worker".to_string());
                        tokio::spawn(async move { connection::serve(&worker, reader, writer, refused).await.finish().await });
                    } else {
                        let worker = worker.clone();
                        serving = Some(tokio::spawn(async move { connection::serve(&worker, reader, writer, None).await }));
                    }
                }
                Err(e) => {
                    // Such as for running out of file descriptors, which may pass.
                    warn!("a hub's connection couldn't be accepted: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            () = tokio::time::sleep_until(alone_since + idle_limit), if serving.is_none() => {
                info!("no hub has connected for {idle_limit:?}, stopping agents");
                break None;
            }
        }
    };
    // Before anything stops, so a hub can't connect to a worker that's stopping, and starts another.
    let _ = std::fs::remove_file(socket);
    drop(listener);
    worker.stop(last).await;
    Ok(())
}

/// Only this user can connect.
fn bind(socket: &Path) -> io::Result<UnixListener> {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    private_files::restrict(socket)?;
    Ok(listener)
}

/// Resolves once the hub connected has stopped sending, and never while none is.
async fn ending(serving: &mut Option<JoinHandle<Closing>>) -> Result<Closing, JoinError> {
    match serving {
        Some(serving) => serving.await,
        None => std::future::pending().await,
    }
}
