//! The requests about a workspace's terminals, whose shells its worker runs. A hub older than terminals refuses each,
//! as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Terminal, TerminalId, WorkspaceId};
use crate::terminal::{Numbering, RowNumber, TerminalBytes, TerminalRowsReply, TerminalSize, Typed};

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

/// Follows the terminal: what it shows, as a [`TerminalUpdate::Screen`], and then each update after it, in order, as
/// `HubFrame::Terminal`s for `subscription_id`, which the client picks, as for a session's, since updates can arrive
/// before the reply. `SessionUnsubscribe` stops it, as it does every kind of subscription. Answers with nothing.
///
/// [`TerminalUpdate::Screen`]: crate::terminal::TerminalUpdate::Screen
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalAttach {
    pub id: TerminalId,
    pub subscription_id: u64,
}

impl Request for TerminalAttach {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalAttach(self)
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

/// What's typed into the terminal, as the bytes a terminal sends for it, such as `\x03` for Control-C. Unlike other
/// requests, it isn't safe to send again: a client sends what's typed once, and drops what's typed while it can't reach
/// the hub, as a terminal over SSH does. A hub takes what one connection types in the order it was sent, unlike other
/// requests, so a client needn't wait for each reply. Answers with nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalInput {
    pub id: TerminalId,
    pub input: TerminalBytes,
}

impl Request for TerminalInput {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalInput(self)
    }
}

/// What's pressed in the terminal, which its worker turns into what the program reads, by the modes the program has
/// set. Like [`TerminalInput`], it isn't safe to send again, and a hub takes what one connection types in the order it
/// was sent, so a client needn't wait for each reply. A hub or worker from before refuses it as `Unsupported`.
/// Answers with nothing.
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

/// Gives the terminal a new size, which every client attached to it is told of, in order with what the program writes,
/// so they all show what it draws for that size. A hub takes the sizes one connection gives in the order they were
/// sent, as it takes what's typed, but not in order with what's typed, so keys typed just after a new size can reach
/// the program before it. Sent again, it sets the same again. Answers with nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalResize {
    pub id: TerminalId,
    pub size: TerminalSize,
}

impl Request for TerminalResize {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::TerminalResize(self)
    }
}

/// Ends the terminal's shell, and the job it runs in front, and closes the terminal. Answers with its record as it last
/// was, and the feed says it's removed. Closing one that's gone is refused with `NotFound`, which a client takes as its
/// being gone.
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
