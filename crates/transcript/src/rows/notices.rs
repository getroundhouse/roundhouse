//! What the app says of a session in a line of its own: that its agent stopped, with what it last printed, and what
//! the hub or the agent's CLI wanted said.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App};
use protocol::event::{ExitReason, NoticeLevel};
use protocol::model::Seq;

use super::{empty, marked_line, muted_line};
use crate::find::{FindLook, Part};
use crate::item_key::ItemKey;

/// What the app says when the agent stopped, and why, with what it last printed, if it stopped unexpectedly.
pub(super) fn agent_exited(
    seq: Seq,
    reason: ExitReason,
    stderr_tail: &[String],
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    match reason {
        ExitReason::Requested => empty(),
        ExitReason::HubRestart => {
            muted_line("The app restarted while the agent was working. Send a message to continue.")
        }
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            const STOPPED: &str = "The agent stopped unexpectedly. ";
            let printed = exit_output(reason, stderr_tail).unwrap_or_default();
            let marks = find.marks(&(ItemKey::Event(seq, 0), Part::Exit), printed).moved(STOPPED.len());
            marked_line(format!("{STOPPED}{printed}"), &marks, cx.theme().danger)
        }
    }
}

/// What the agent last printed, which the app says when it stopped unexpectedly.
pub(super) fn exit_output(reason: ExitReason, stderr_tail: &[String]) -> Option<&str> {
    match reason {
        ExitReason::Requested | ExitReason::HubRestart => None,
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            stderr_tail.last().map(String::as_str)
        }
    }
}

/// Something the hub or the agent's CLI wanted said, in the colour of how much it matters, with its matches.
pub(super) fn notice(seq: Seq, level: NoticeLevel, message: &str, find: &FindLook, cx: &App) -> AnyElement {
    let color = match level {
        NoticeLevel::Error => cx.theme().danger,
        NoticeLevel::Warning => cx.theme().warning,
        NoticeLevel::Info | NoticeLevel::Unknown => cx.theme().muted_foreground,
    };
    let marks = find.marks(&(ItemKey::Event(seq, 0), Part::Notice), message);
    marked_line(message.to_string(), &marks, color)
}
