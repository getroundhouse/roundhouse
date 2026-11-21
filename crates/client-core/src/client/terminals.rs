//! What a client asks of its hub about a workspace's terminals, as `protocol::client::terminals` says.

use protocol::client::Request as _;
use protocol::client::terminals::{
    TerminalAttach, TerminalBounds, TerminalClose, TerminalFit, TerminalInput, TerminalOpen, TerminalResize,
    TerminalRows, TerminalText, TerminalTyped, TerminalWatch,
};
use protocol::model::{Terminal, TerminalId, WorkspaceId};
use protocol::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalBytes, TerminalFitReply, TerminalPoint, TerminalRange,
    TerminalRowsReply, TerminalSize, TerminalTextReply, Typed,
};

use super::{Client, Result, Subscription};

impl Client {
    /// Opens a terminal in the workspace, with an ID new to it, so opening it again opens it once.
    pub fn terminal_open(&self, workspace_id: &WorkspaceId, id: &TerminalId, size: TerminalSize) -> Result<Terminal> {
        self.call(TerminalOpen { workspace_id: workspace_id.clone(), id: id.clone(), size })
    }

    /// Follows the terminal: what it shows, then each update after it, as `SubscriptionItem::Terminal`s.
    pub fn terminal_attach(&self, id: &TerminalId) -> Result<Subscription> {
        self.subscribe(|subscription_id| TerminalAttach { id: id.clone(), subscription_id })
    }

    /// Sends what's typed without waiting for the hub's reply, so a burst of keys isn't held to one a round trip. It
    /// reaches the terminal in the order it was sent, since a connection writes one request at a time, and the hub and
    /// the worker pass what's typed on in that order. It's sent once: what's typed while there's no connection fails
    /// with `Disconnected`, and is dropped.
    pub fn terminal_input(&self, id: &TerminalId, input: &[u8]) -> Result<()> {
        let typed = TerminalInput { id: id.clone(), input: TerminalBytes::from(input) };
        self.inner.send_only(typed.into_request())
    }

    /// Gives the terminal a new size, sent as what's typed is, without waiting for the reply. The sizes one connection
    /// gives take effect in the order they were sent, though not in order with what's typed.
    pub fn terminal_resize(&self, id: &TerminalId, size: TerminalSize) -> Result<()> {
        self.inner.send_only(TerminalResize { id: id.clone(), size }.into_request())
    }

    /// Watches the terminal: a picture of what it shows, then what it lacks of the newest picture whenever the last
    /// has been written, as `SubscriptionItem::TerminalPicture`s, and its shell's end as a `SubscriptionItem::Terminal`.
    pub fn terminal_watch(&self, id: &TerminalId) -> Result<Subscription> {
        self.subscribe(|subscription_id| TerminalWatch { id: id.clone(), subscription_id })
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

    /// What's pressed, which the terminal's worker turns into what the program reads, by the modes it set, sent as
    /// [`Client::terminal_input`] sends what's typed: without waiting for the reply, in order, and once.
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

    /// Ends the terminal's shell and closes the terminal, answering with its record as it last was.
    pub fn terminal_close(&self, id: &TerminalId) -> Result<Terminal> {
        self.call(TerminalClose { id: id.clone() })
    }
}
