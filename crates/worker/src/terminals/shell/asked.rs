//! What the hub asks of what a terminal shows, which only the worker's copy of it knows: rows of its history, the ends
//! of a word or a line, and the text of cells. Answered on the terminal's thread, between what its program writes.

use protocol::client::{ApiError, ErrorCode};
use protocol::terminal::{TerminalBoundsReply, TerminalRowsReply, TerminalTextReply};
use protocol::worker::{TerminalBounds, TerminalRows, TerminalText};
use terminal_state::{Mirror, MirrorError};
use tokio::sync::oneshot;

pub(in crate::terminals) enum Asked {
    Rows(TerminalRows, oneshot::Sender<Result<TerminalRowsReply, ApiError>>),
    Bounds(TerminalBounds, oneshot::Sender<Result<TerminalBoundsReply, ApiError>>),
    Text(TerminalText, oneshot::Sender<Result<TerminalTextReply, ApiError>>),
}

impl Asked {
    pub(super) fn answer(self, mirror: &mut Mirror) {
        match self {
            Self::Rows(asked, reply) => {
                let rows = mirror.history_rows(asked.numbering, asked.from, asked.count);
                let _ = reply.send(rows.map_err(|e| failed("reading the terminal's rows", &e)));
            }
            Self::Bounds(asked, reply) => {
                let bounds = mirror.bounds(asked.numbering, asked.at, asked.of);
                let _ = reply.send(bounds.map_err(|e| failed("finding where a word ends", &e)));
            }
            Self::Text(asked, reply) => {
                let text = mirror.text(asked.numbering, asked.range, asked.rectangle);
                let _ = reply.send(text.map_err(|e| failed("copying the terminal's text", &e)));
            }
        }
    }
}

fn failed(what: &str, e: &MirrorError) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("{what}: {e}"))
}
