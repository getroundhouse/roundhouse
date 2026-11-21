//! What the hub tells a terminal to do with what it shows: turn what's pressed into what the program reads, and take
//! a new size, keeping the cells it names.

use protocol::client::{ApiError, ErrorCode};
use protocol::terminal::{TerminalFitReply, TerminalSize, TerminalUpdate, Typed};
use protocol::worker::TerminalFit;
use terminal_state::Mirror;
use tracing::warn;

use super::serving::{Shell, pty_size};

impl Shell {
    /// Writes what the program reads for what was pressed, by the modes it set in what it wrote before.
    pub(super) fn typed(&self, mirror: &mut Mirror, typed: &Typed) {
        match mirror.typed(typed) {
            // Its writer has gone only once its program has, and the hub hears that next.
            Ok(bytes) => drop(self.to_program.send(bytes)),
            Err(e) => warn!("couldn't say what terminal {} reads for {typed:?}: {e}", self.terminal),
        }
    }

    /// Gives the worker's copy the new size, and the PTY, which tells the program, and says so in order with the
    /// output. The same size again changes nothing, and so does one either can't take, so the two never differ.
    pub(super) async fn resize(&mut self, mirror: &mut Mirror, size: TerminalSize) {
        if size == self.size {
            return;
        }
        if let Err(e) = mirror.resize(size) {
            warn!("couldn't resize the copy of terminal {}: {e}", self.terminal);
            return;
        }
        if let Err(e) = self.master.resize(pty_size(size)) {
            warn!("couldn't resize terminal {}: {e}", self.terminal);
            if let Err(e) = mirror.resize(self.size) {
                warn!("couldn't put the copy of terminal {} back to its size: {e}", self.terminal);
            }
            // Its rows are in a new numbering, which a picture says.
            self.watching.changed(mirror);
            return;
        }
        self.size = size;
        self.watching.changed(mirror);
        self.send(TerminalUpdate::Resized { size }).await;
    }

    /// Gives the terminal the new size, as [`Shell::resize`] does, and says where each cell the hub named went, in
    /// the numbering its rows are in after.
    pub(super) async fn fit(&mut self, mirror: &mut Mirror, fit: TerminalFit) -> Result<TerminalFitReply, ApiError> {
        let terminal = self.terminal.clone();
        let failed = move |e| ApiError::new(ErrorCode::Worker, format!("keeping cells of terminal {terminal}: {e}"));
        let tracked = mirror.track(fit.numbering, &fit.keep).map_err(failed.clone())?;
        self.resize(mirror, fit.size).await;
        mirror.tracked(&tracked).map_err(failed)
    }
}
