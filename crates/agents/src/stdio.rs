use std::collections::VecDeque;
use std::process::ExitStatus;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::ChildStderr;
use tracing::warn;

const STDERR_TAIL_LINES: usize = 20;

/// Reads bytes rather than text: one line of invalid UTF-8 mustn't end the whole stream, which would
/// leave the agent blocked and the session stuck.
pub(crate) async fn next_line<R: AsyncBufRead + Unpin>(reader: &mut R, buffer: &mut Vec<u8>) -> Option<String> {
    buffer.clear();
    match reader.read_until(b'\n', buffer).await {
        Ok(0) => None,
        Ok(_) => Some(String::from_utf8_lossy(buffer).trim_end_matches(['\n', '\r']).to_string()),
        Err(e) => {
            warn!("reading agent output failed: {e}");
            None
        }
    }
}

pub(crate) async fn read_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    tail.into()
}

#[cfg(unix)]
pub(crate) fn signal_of(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

/// Windows reports exit codes, not signals.
#[cfg(windows)]
pub(crate) fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}
