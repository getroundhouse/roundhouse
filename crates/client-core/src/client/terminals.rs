//! What a client asks of its hub about a workspace's terminals, as `protocol::client::terminals` says.

use protocol::client::Request as _;
use protocol::client::terminals::{TerminalAttach, TerminalClose, TerminalInput, TerminalOpen, TerminalResize};
use protocol::model::{Terminal, TerminalId, WorkspaceId};
use protocol::terminal::{TerminalBytes, TerminalSize};

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

    /// Ends the terminal's shell and closes the terminal, answering with its record as it last was.
    pub fn terminal_close(&self, id: &TerminalId) -> Result<Terminal> {
        self.call(TerminalClose { id: id.clone() })
    }
}
