//! What the hub tells a terminal to do with what it shows: turn what's pressed into what the program reads, and take
//! a new size, keeping the cells it names.

use protocol::client::{ApiError, ErrorCode};
use protocol::terminal::{TerminalFitReply, TerminalSize, Typed};
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
            Err(e) => {
                warn!(
                    name: "terminal.typed_unread",
                    terminal = %self.terminal,
                    error = %e,
                    "Couldn't say what a terminal reads for what was typed"
                );
            }
        }
    }

    /// Gives the worker's copy the new size, and the PTY, which tells the program. The same size again changes
    /// nothing, and so does one either can't take, so the two never differ.
    fn resize(&mut self, mirror: &mut Mirror, size: TerminalSize) {
        if size == self.size {
            return;
        }
        if let Err(e) = mirror.resize(size) {
            warn!(
                name: "terminal.copy_unresized",
                terminal = %self.terminal,
                error = %e,
                "Couldn't resize the copy of a terminal"
            );
            return;
        }
        if let Err(e) = self.master.resize(pty_size(size)) {
            warn!(name: "terminal.unresized", terminal = %self.terminal, error = %e, "Couldn't resize a terminal");
            if let Err(e) = mirror.resize(self.size) {
                warn!(
                    name: "terminal.copy_not_put_back",
                    terminal = %self.terminal,
                    error = %e,
                    "Couldn't put the copy of a terminal back to its size"
                );
            }
            // Its rows are in a new numbering, which a picture says.
            self.watching.changed(mirror);
            return;
        }
        self.size = size;
        self.watching.changed(mirror);
    }

    /// Gives the terminal the new size, as [`Shell::resize`] does, and says where each cell the hub named went, in
    /// the numbering its rows are in after.
    pub(super) fn fit(&mut self, mirror: &mut Mirror, fit: &TerminalFit) -> Result<TerminalFitReply, ApiError> {
        let terminal = self.terminal.clone();
        let failed = move |e| ApiError::new(ErrorCode::Worker, format!("keeping cells of terminal {terminal}: {e}"));
        let tracked = mirror.track(fit.numbering, &fit.keep).map_err(failed.clone())?;
        self.resize(mirror, fit.size);
        mirror.tracked(&tracked).map_err(failed)
    }
}
