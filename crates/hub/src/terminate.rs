//! What asks a hub to stop from outside, besides Ctrl-C: `SIGTERM`, or on Windows, Ctrl-Break or its console closing.

use std::io;

#[cfg(unix)]
pub(crate) struct Terminate(tokio::signal::unix::Signal);

#[cfg(unix)]
impl Terminate {
    pub(crate) fn listen() -> io::Result<Self> {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map(Self)
    }

    pub(crate) async fn asked(&mut self) {
        self.0.recv().await;
    }
}

#[cfg(windows)]
pub(crate) struct Terminate {
    ctrl_break: tokio::signal::windows::CtrlBreak,
    ctrl_close: tokio::signal::windows::CtrlClose,
}

#[cfg(windows)]
impl Terminate {
    pub(crate) fn listen() -> io::Result<Self> {
        Ok(Self {
            ctrl_break: tokio::signal::windows::ctrl_break()?,
            ctrl_close: tokio::signal::windows::ctrl_close()?,
        })
    }

    pub(crate) async fn asked(&mut self) {
        tokio::select! {
            _ = self.ctrl_break.recv() => {}
            _ = self.ctrl_close.recv() => {}
        }
    }
}
