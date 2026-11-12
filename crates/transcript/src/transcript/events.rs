//! What a transcript tells whoever shows it.

use client_core::FileToSend;
use gpui_kit::FocusHandle;
use protocol::model::ToolUseId;
use protocol::prompt::Prompt;

/// Things the owner of a transcript has to deal with.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    /// Something failed that the user should hear about.
    Error(String),
    /// A prompt couldn't be sent, so what was typed and attached is handed back, to be offered again.
    NotSent { message: String, prompt: Prompt<FileToSend> },
    /// The agent finished a turn, so anything derived from the worktree may be out of date.
    TurnEnded,
    /// The question in the composer's place changed: one came, or the one there was answered. `left` is the focus of
    /// the one that went, if one did, so focus that was in it can move to what's drawn now.
    Asking { left: Option<FocusHandle> },
    /// What's chosen or typed in answer to the question changed, so the hub can keep it: within a moment while it's
    /// typed, or at once as something's chosen or it's answered. `QuestionPanel::so_far` says what it is now.
    Answering { at_once: bool },
    /// Someone asked to see the work of the subagent the call `call` started, in the subagent tab. `title` is what the
    /// call says it's for, for a subagent whose record hasn't come.
    OpenSubagent { call: ToolUseId, title: String },
    /// The bar for finding text in the chat closed, so focus can go back where it was.
    FindClosed,
}
