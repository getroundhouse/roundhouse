//! What a client reads of a terminal's copy, which only its worker has: rows of its history, the ends of a word or a
//! line, and the text of cells. The hub keeps none of them, so it asks the worker, and waits only so long.

use std::time::Duration;

use protocol::client::terminals::{TerminalBounds, TerminalRows, TerminalText};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::TerminalId;
use protocol::terminal::{TerminalBoundsReply, TerminalRowsReply, TerminalTextReply};
use protocol::worker::{self, Request};

use crate::hub::Hub;

/// How long the hub waits for a worker to answer what's read of a terminal. Its terminal's thread answers between
/// reads of what the program writes, each a fraction of a millisecond, so a worker that takes this long is stuck, and
/// the client is better told so than left waiting, since it can ask again.
const READ_WITHIN: Duration = Duration::from_secs(10);

impl Hub {
    /// Rows of an open terminal's history.
    pub(crate) async fn terminal_rows(&self, asked: TerminalRows) -> Result<TerminalRowsReply, ApiError> {
        let TerminalRows { id, numbering, from, count } = asked;
        self.read_terminal(&id, worker::TerminalRows { terminal: id.clone(), numbering, from, count }).await
    }

    /// The ends of a word or a line in an open terminal.
    pub(crate) async fn terminal_bounds(&self, asked: TerminalBounds) -> Result<TerminalBoundsReply, ApiError> {
        let TerminalBounds { id, numbering, at, of } = asked;
        self.read_terminal(&id, worker::TerminalBounds { terminal: id.clone(), numbering, at, of }).await
    }

    /// The text of cells in an open terminal, a page at a time.
    pub(crate) async fn terminal_text(&self, asked: TerminalText) -> Result<TerminalTextReply, ApiError> {
        let TerminalText { id, numbering, range, rectangle } = asked;
        self.read_terminal(&id, worker::TerminalText { terminal: id.clone(), numbering, range, rectangle }).await
    }

    async fn read_terminal<R: Request>(&self, id: &TerminalId, request: R) -> Result<R::Reply, ApiError> {
        let (_, link) = self.open_terminal(id)?;
        within_a_read(id, link.call(request)).await
    }
}

/// The worker's answer about terminal `id`, or an error once it has taken longer than [`READ_WITHIN`].
pub(super) async fn within_a_read<T>(
    id: &TerminalId,
    answer: impl Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
    tokio::time::timeout(READ_WITHIN, answer).await.unwrap_or_else(|_| {
        Err(ApiError::new(ErrorCode::Worker, format!("terminal {id}'s worker didn't answer in {READ_WITHIN:?}")))
    })
}
