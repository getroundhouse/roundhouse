//! What a hub asks a worker about the terminals it runs: a shell each, in a PTY, in the worktree, whose output the
//! worker keeps what the terminal shows from. A worker older than terminals refuses each as unsupported.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::TerminalId;
use crate::terminal::{TerminalBytes, TerminalScreen, TerminalSize, UpdateIndex};

/// Starts a shell in `cwd`, in a terminal of `size`. Starting one that runs already changes nothing, so a hub can send
/// it again. What the shell writes comes as [`WorkerFrame::Terminal`](super::WorkerFrame::Terminal)s.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalStart {
    pub terminal: TerminalId,
    pub size: TerminalSize,
    /// Where the shell starts, which is the worktree, in full, as an agent's `cwd` is.
    pub cwd: String,
    /// Set in the shell's environment, along with the worker's own, as for an agent.
    pub env: Vec<(String, String)>,
    /// When the hub opened it, by the hub's clock, which the worker gives back in [`RunningTerminal`], so a hub that
    /// takes over the worker orders its terminals by the same clock as the ones it opens itself.
    pub opened_at_ms: i64,
}

impl Request for TerminalStart {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TerminalStart(self)
    }
}

/// Writes what was typed into the terminal, for its program to read. A worker takes what's typed in the order it was
/// sent, unlike other requests, so a hub needn't wait for each reply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalInput {
    pub terminal: TerminalId,
    pub input: TerminalBytes,
}

impl Request for TerminalInput {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TerminalInput(self)
    }
}

/// Gives the terminal a new size, which the worker says in order with what the program writes, as a
/// [`TerminalUpdate::Resized`](crate::terminal::TerminalUpdate::Resized). A worker takes the sizes it's given in the
/// order they were sent, as it takes what's typed, but not in order with what's typed, which reaches the program by
/// another way, so keys typed just after a new size can reach it first. Sent again, it sets the same again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalResize {
    pub terminal: TerminalId,
    pub size: TerminalSize,
}

impl Request for TerminalResize {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TerminalResize(self)
    }
}

/// What the terminal shows now, for a client that attaches, or one that fell behind.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalShow {
    pub terminal: TerminalId,
}

impl Request for TerminalShow {
    type Reply = TerminalShown;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TerminalShow(self)
    }
}

/// What a terminal shows, and which of its updates come after that, so a hub sends a client only those, whichever
/// arrives first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalShown {
    pub screen: TerminalScreen,
    /// The `at` of the first update the screen doesn't take in, which is how many it does.
    pub next: UpdateIndex,
}

impl Reply for TerminalShown {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::TerminalShown(shown) = response else { return Err(response) };
        Ok(shown)
    }
}

/// Ends the terminal's shell, and the job it runs in front. Ending one that's gone changes nothing, so a hub can send
/// it again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TerminalStop {
    pub terminal: TerminalId,
}

impl Request for TerminalStop {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TerminalStop(self)
    }
}

/// Every terminal the worker runs, for a hub that connects to a worker another hub started, so it has their records
/// again.
#[derive(Clone, Copy, Debug)]
pub struct RunningTerminals;

impl Request for RunningTerminals {
    type Reply = RunningTerminalsReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::RunningTerminals
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunningTerminalsReply {
    pub terminals: Vec<RunningTerminal>,
}

impl Reply for RunningTerminalsReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::RunningTerminals(running) = response else { return Err(response) };
        Ok(running)
    }
}

/// A terminal a worker runs, as [`RunningTerminalsReply`] lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RunningTerminal {
    pub terminal: TerminalId,
    /// As its [`TerminalStart`] gave it, so its record keeps its place among the workspace's terminals.
    pub opened_at_ms: i64,
}
