//! The requests about a workspace's terminals, whose shells its worker runs. A hub older than terminals refuses each,
//! as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Terminal, TerminalId, WorkspaceId};
use crate::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalFitReply, TerminalPoint, TerminalRange,
    TerminalRowsReply, TerminalSize, TerminalTextReply, Typed,
};

/// Opens a terminal in the workspace: a shell in its worktree, of `size`, which the workspace's worker runs, starting
/// the worker if none is. Answers with the terminal's record, which the feed also announces. Opening one with the same
/// `id` again answers with the one open, so sending it again can't open two, and one that's another workspace's is
/// refused with `Conflict`. Refused unless the workspace is ready.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalOpen {
    pub workspace_id: WorkspaceId,
    pub id: TerminalId,
    pub size: TerminalSize,
}

impl Request for TerminalOpen {
    type Reply = Terminal;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalOpen(self)
    }
}

impl Reply for Terminal {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Terminal(terminal) = response else { return Err(response) };
        Ok(terminal)
    }
}

/// Watches the terminal: what it shows, as frames of `HubFrame::TerminalPicture` for `subscription_id`, which the
/// client picks, as for a session's, since a frame can arrive before the reply. The first is fresh and whole, from the
/// hub's copy of the screen if it has one, and each after it has only what the client lacks of the newest the hub has,
/// with at most one frame on its way at a time, so a client on a slow link is sent the newest of what the terminal
/// shows rather than all that came between. Its shell's end comes as a `HubFrame::Terminal` with
/// [`TerminalUpdate::Exited`], after which nothing more does. `SessionUnsubscribe` stops it. Answers with nothing.
///
/// [`TerminalUpdate::Exited`]: crate::terminal::TerminalUpdate::Exited
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalWatch {
    pub id: TerminalId,
    pub subscription_id: u64,
}

impl Request for TerminalWatch {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalWatch(self)
    }
}

/// Rows of the terminal's history, by their numbers in `numbering`, as its worker sends them, for a client scrolled
/// back past the rows it holds. The hub keeps none, and asks the worker. In another numbering, the reply has no rows,
/// and says which numbering the terminal's rows are in now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalRows {
    pub id: TerminalId,
    pub numbering: Numbering,
    pub from: RowNumber,
    pub count: u32,
}

impl Request for TerminalRows {
    type Reply = TerminalRowsReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalRows(self)
    }
}

impl Reply for TerminalRowsReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::TerminalRows(rows) = response else { return Err(response) };
        Ok(rows)
    }
}

/// The ends of the word or the line at a cell, by Ghostty's rules, as a double or a triple click selects. The hub
/// asks the worker, which has the only copy of the terminal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalBounds {
    pub id: TerminalId,
    pub numbering: Numbering,
    pub at: TerminalPoint,
    pub of: Bounds,
}

impl Request for TerminalBounds {
    type Reply = TerminalBoundsReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalBounds(self)
    }
}

impl Reply for TerminalBoundsReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::TerminalBounds(bounds) = response else { return Err(response) };
        Ok(bounds)
    }
}

/// The text of the cells in `range`, or of the columns between its two cells in each of its rows for a `rectangle`,
/// as Ghostty copies it, a page at a time, as [`TerminalTextReply`] says. The hub asks the worker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalText {
    pub id: TerminalId,
    pub numbering: Numbering,
    pub range: TerminalRange,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rectangle: bool,
}

impl Request for TerminalText {
    type Reply = TerminalTextReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalText(self)
    }
}

impl Reply for TerminalTextReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::TerminalText(text) = response else { return Err(response) };
        Ok(text)
    }
}

/// What's pressed in the terminal, which its worker turns into what the program reads, by the modes the program has
/// set. Unlike other requests, it isn't safe to send again: a client sends what's pressed once, and drops what's
/// pressed while it can't reach the hub, as a terminal over SSH does. A hub takes what one connection types in the
/// order it was sent, unlike other requests, so a client needn't wait for each reply. Answers with nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalTyped {
    pub id: TerminalId,
    pub typed: Typed,
}

impl Request for TerminalTyped {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalTyped(self)
    }
}

/// Gives the terminal a new size, keeping the cells in `keep`, which are in `numbering`, as [`TerminalFitReply`] says.
/// Every client watching is sent what it shows at that size. A hub takes it in the order one connection sent it, with
/// what's pressed, which the worker takes in the same order, so a key pressed after it reaches a program that has it.
/// Sent again, it sets the same again, and starts no new numbering. Refused with more than
/// [`MOST_KEPT_POINTS`](crate::terminal::MOST_KEPT_POINTS) cells to keep.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalFit {
    pub id: TerminalId,
    pub size: TerminalSize,
    pub numbering: Numbering,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep: Vec<TerminalPoint>,
}

impl Request for TerminalFit {
    type Reply = TerminalFitReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalFit(self)
    }
}

impl Reply for TerminalFitReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::TerminalFit(fit) = response else { return Err(response) };
        Ok(fit)
    }
}

/// Ends the terminal's shell, and the job it runs in front, which closes the terminal. Answers with its record as it last
/// was once its worker has been told, without waiting for the shell to end, and the feed says it's removed once the
/// shell has ended. Closing one that's gone is refused with `NotFound`, which a client takes as its being gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalClose {
    pub id: TerminalId,
}

impl Request for TerminalClose {
    type Reply = Terminal;

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalClose(self)
    }
}
