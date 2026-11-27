//! What a client asks of its hub about a workspace's terminals, as `protocol::client::terminals` says.

use protocol::client::Request as _;
use protocol::client::terminals::{
    TerminalBounds, TerminalClose, TerminalFit, TerminalOpen, TerminalRows, TerminalText, TerminalTyped,
};
use protocol::model::{Terminal, TerminalId, WorkspaceId};
use protocol::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalFitReply, TerminalPoint, TerminalRange,
    TerminalRowsReply, TerminalSize, TerminalTextReply, Typed,
};

use super::{Connection, Result};

impl Connection {
    /// Opens a terminal in the workspace, with an ID new to it, so opening it again opens it once.
    pub fn terminal_open(&self, workspace_id: &WorkspaceId, id: &TerminalId, size: TerminalSize) -> Result<Terminal> {
        self.call(TerminalOpen { workspace_id: workspace_id.clone(), id: id.clone(), size })
    }

    /// Rows of the terminal's history from `from`, by their numbers in `numbering`, for a view scrolled back past the
    /// rows it holds. In another numbering, the reply has none, and says the one its rows are in now.
    pub fn terminal_rows(
        &self,
        id: &TerminalId,
        numbering: Numbering,
        from: RowNumber,
        count: u32,
    ) -> Result<TerminalRowsReply> {
        self.call(TerminalRows { id: id.clone(), numbering, from, count })
    }

    /// What's pressed, which the terminal's worker turns into what the program reads, by the modes it set, sent
    /// without waiting for the hub's reply, so a burst of keys isn't held to one a round trip. It reaches the terminal
    /// in the order it was sent, since a connection writes one request at a time, and the hub and the worker pass
    /// what's pressed on in that order. It's sent once: what's pressed while there's no connection fails with
    /// `Disconnected`, and is dropped.
    pub fn terminal_typed(&self, id: &TerminalId, typed: Typed) -> Result<()> {
        self.inner.send_only(TerminalTyped { id: id.clone(), typed }.into_request())
    }

    /// Gives the terminal a new size, in order with what's pressed, keeping the cells in `keep`, which are in
    /// `numbering`. The reply says where each went in the numbering the new size starts.
    pub fn terminal_fit(
        &self,
        id: &TerminalId,
        size: TerminalSize,
        numbering: Numbering,
        keep: Vec<TerminalPoint>,
    ) -> Result<TerminalFitReply> {
        self.call(TerminalFit { id: id.clone(), size, numbering, keep })
    }

    /// The ends of the word or the line at `at`, as a double or a triple click selects them.
    pub fn terminal_bounds(
        &self,
        id: &TerminalId,
        numbering: Numbering,
        at: TerminalPoint,
        of: Bounds,
    ) -> Result<TerminalBoundsReply> {
        self.call(TerminalBounds { id: id.clone(), numbering, at, of })
    }

    /// A page of the text of the cells in `range`, or of the columns between its two cells in each of its rows for a
    /// `rectangle`, as the terminal copies it. The reply says where the rest starts.
    pub fn terminal_text(
        &self,
        id: &TerminalId,
        numbering: Numbering,
        range: TerminalRange,
        rectangle: bool,
    ) -> Result<TerminalTextReply> {
        self.call(TerminalText { id: id.clone(), numbering, range, rectangle })
    }

    /// Ends the terminal's shell, which closes the terminal, answering with its record as it last was without waiting for
    /// the shell to end, as [`TerminalClose`] says.
    pub fn terminal_close(&self, id: &TerminalId) -> Result<Terminal> {
        self.call(TerminalClose { id: id.clone() })
    }
}
