//! Workspaces whose order changes as they're worked in, as the sidebar and Home list them: three to put in order,
//! working in one, and the order their rows are drawn in.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Pixels, TestAppContext};
use protocol::model::{Seq, Session, SessionId, WorkspaceId};

use super::{Stack, Ui};

/// A window with three workspaces in one repository, made in the order they're returned, each with its chat.
pub(crate) fn three_workspaces<'a>(
    cx: &'a mut TestAppContext,
    stack: &Stack,
) -> (Ui<'a>, [WorkspaceId; 3], [SessionId; 3]) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let mut made = Vec::new();
    for _ in 0..3 {
        let workspace = ui.new_workspace();
        made.push((workspace, ui.active_chat()));
    }
    let (workspaces, chats): (Vec<_>, Vec<_>) = made.into_iter().unzip();
    (ui, workspaces.try_into().unwrap(), chats.try_into().unwrap())
}

/// Sends `chat` its first prompt, as the person would, and waits until the app has heard its agent reply, so nothing
/// in the window is still at work, and heard from its workspace's record that it was worked in then.
pub(crate) fn worked_in(ui: &mut Ui, stack: &Stack, chat: &SessionId) {
    stack.client().session_prompt(chat, "say:done", None).unwrap();
    let replied = |c: &Session| c.user_active_at_ms.is_some() && c.reply_seq != Seq::default() && !c.status.in_turn();
    let caught_up = |ui: &mut Ui| {
        let view = ui.view();
        let Some(c) = view.records().session(chat).filter(|c| replied(c)) else { return false };
        view.records().workspace(&c.workspace_id).is_some_and(|w| w.user_active_at_ms == c.user_active_at_ms)
    };
    ui.wait("the reply", caught_up);
}

/// How far down the window the last frame drew `row`.
pub(crate) fn top(ui: &mut Ui, row: ElementId) -> Pixels {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(row).bounds().top()).unwrap()
}

/// The workspaces from the top of a list down, by their rows in it, which `row` names.
pub(crate) fn order<const N: usize>(
    ui: &mut Ui,
    row: fn(&WorkspaceId) -> ElementId,
    workspaces: &[WorkspaceId; N],
) -> Vec<WorkspaceId> {
    let mut drawn: Vec<(Pixels, WorkspaceId)> = workspaces.iter().map(|id| (top(ui, row(id)), id.clone())).collect();
    drawn.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    drawn.into_iter().map(|(_, id)| id).collect()
}
