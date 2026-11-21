//! The command the hub runs to reach a sandbox's worker: it runs the worker's `--attach` in the sandbox, as the worker's
//! user, and streams this command's stdin to it and its stdout back, so the hub's worker stream runs through it. What
//! the worker logs stays in the sandbox, as [`guest::WORKER_LOG`] says why.

use std::process::ExitCode;

use microsandbox::{ExecEvent, Sandbox};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::guest;

/// Until the worker's `--attach` exits, with its exit code, or the hub closes this command's stdin.
pub(crate) async fn connect(sandbox_name: &str) -> Result<ExitCode, String> {
    let sandbox = Sandbox::get(sandbox_name)
        .await
        .map_err(|e| format!("finding sandbox {sandbox_name}: {e}"))?
        .connect()
        .await
        .map_err(|e| format!("connecting to sandbox {sandbox_name}: {e}"))?;
    let (program, args) = guest::worker_command();
    let mut attached = sandbox
        .exec_stream_with(program, |options| {
            let options = options.args(args).user(guest::USER).stdin_pipe();
            guest::worker_env().into_iter().fold(options, |options, (key, value)| options.env(key, value))
        })
        .await
        .map_err(|e| format!("running the worker in sandbox {sandbox_name}: {e}"))?;
    let sink = attached.take_stdin().ok_or("the worker in the sandbox took no input")?;
    let sending = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0; 1 << 16];
        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if sink.write(&buffer[..read]).await.is_err() {
                        break;
                    }
                }
            }
        }
        // The hub has closed the stream, so the worker hears it has gone.
        let _ = sink.close().await;
    });
    let (mut stdout, mut stderr) = (tokio::io::stdout(), tokio::io::stderr());
    let code = loop {
        match attached.recv().await {
            Some(ExecEvent::Stdout(bytes)) => {
                if stdout.write_all(&bytes).await.is_err() || stdout.flush().await.is_err() {
                    break 1;
                }
            }
            Some(ExecEvent::Stderr(bytes)) => {
                let _ = stderr.write_all(&bytes).await;
            }
            Some(ExecEvent::Exited { code }) => break code,
            Some(_) => {}
            None => break 1,
        }
    };
    sending.abort();
    sandbox.detach().await;
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}
