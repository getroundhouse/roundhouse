//! The threads a terminal's shell is served by, beside the one that keeps what it shows: one reads what its program
//! writes, one writes what's typed, and one waits for the shell to end.

use std::io::{Read, Write};

use portable_pty::Child;
use protocol::terminal::TerminalExit;
use tokio::sync::{mpsc, oneshot};

/// The most a read takes from the PTY at once.
const READ_BYTES: usize = 64 * 1024;

pub(super) fn read_output(mut reader: Box<dyn Read + Send>, output: &mpsc::Sender<Vec<u8>>) {
    let mut buffer = vec![0; READ_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                if output.blocking_send(buffer[..read].to_vec()).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // Linux says so once every program in the terminal has closed it.
            Err(_) => return,
        }
    }
}

/// Until the terminal has gone, and with it every way to type.
pub(super) fn write_typed(mut writer: Box<dyn Write + Send>, to_type: &std::sync::mpsc::Receiver<Vec<u8>>) {
    for typed in to_type {
        if writer.write_all(&typed).and_then(|()| writer.flush()).is_err() {
            return;
        }
    }
}

pub(super) fn wait(mut child: Box<dyn Child + Send + Sync>, exited: oneshot::Sender<TerminalExit>) {
    let status = child.wait();
    let code = status.ok().filter(|status| status.signal().is_none()).map(|status| status.exit_code().cast_signed());
    let _ = exited.send(TerminalExit { code });
}
