//! What the app says of a session in a line of its own: that its agent stopped, with what it last printed, and what
//! the hub or the agent's CLI wanted said.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _, div};
use protocol::event::{ExitReason, NoticeLevel};
use protocol::model::Seq;

use super::{Shared, empty, from_blocks, marked_line, muted_line, row};
use crate::find::{FindLook, Part};
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};

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

/// Something the hub or the agent's CLI wanted said, in the colour of how much it matters, with its matches. It's
/// Markdown, as what the CLI says can be, such as the model it was set to, in code.
pub(super) fn notice(
    seq: Seq,
    level: NoticeLevel,
    message: &str,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let color = match level {
        NoticeLevel::Error => cx.theme().danger,
        NoticeLevel::Warning => cx.theme().warning,
        NoticeLevel::Info | NoticeLevel::Unknown => cx.theme().muted_foreground,
    };
    let id = (ItemKey::Event(seq, 0), Part::Notice);
    let said = from_blocks(&Written::found(TextAt::only(seq, 0), &id, find, message), None, shared);
    row().child(div().text_sm().text_color(color).child(said)).into_any_element()
}
