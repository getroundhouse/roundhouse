//! Workspaces whose order changes as they're worked in, as the sidebar and Home list them: three to put in order, more
//! than the sidebar has room for, working in one, and the order their rows are drawn in.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Pixels, TestAppContext};
use protocol::model::{Session, SessionId, SetupStatus, Workspace, WorkspaceId};

use super::{Stack, Ui, wait_until};

/// More workspaces than the sidebar has room for in the window, which shows 15, and few enough that a test makes
/// them quickly.
pub(crate) const MORE_THAN_FIT: usize = 24;

/// Waits until the clock has gone past every time the app has heard a workspace was made or worked in, so whatever is
/// made or done in one next comes after all of them, as what a person does next would. The hub stamps those times from
/// the machine's clock, which reads the same for two things done within a millisecond, and goes back when the machine
/// sets it back, and a workspace whose time ties with another's, or comes before it, would be listed below it.
pub(crate) fn after_every_workspace(ui: &Ui) {
    let latest = ui.view().records().workspaces().iter().map(Workspace::last_active_at_ms).max().unwrap_or(i64::MIN);
    wait_until("the clock to pass every workspace's last activity", || hub::store::now_ms() > latest);
}

/// When each of `workspaces` was last made or worked in, as the app has heard, in the same order.
fn last_active(ui: &Ui, workspaces: &[WorkspaceId]) -> Vec<i64> {
    let view = ui.view();
    let at = |id: &WorkspaceId| view.records().workspace(id).map(Workspace::last_active_at_ms);
    workspaces.iter().map(|id| at(id).unwrap_or_else(|| panic!("the app has no record of {id:?}"))).collect()
}

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
    let times = last_active(&ui, &workspaces);
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "each made after the one before: {times:?}");
    (ui, workspaces.try_into().unwrap(), chats.try_into().unwrap())
}

/// Waits until every workspace's setup has ended. A workspace's mark moves while its setup is running, and the hub
/// starts it a moment after it says the workspace is ready, so until it has ended, a mark can still ask for a frame
/// after every frame for as long as motion is allowed, and nothing could be seen to come to rest.
pub(crate) fn wait_out_setup(ui: &mut Ui) {
    let ended = |w: &Workspace| matches!(w.setup, SetupStatus::Succeeded | SetupStatus::Failed { .. });
    ui.wait("every workspace's setup to end", |ui| ui.view().records().workspaces().iter().all(ended));
}

/// Sends `chat` its first prompt, as the person would, once the clock has passed every workspace's last activity, and
/// waits until the app has heard its agent reply, so nothing in the window is still at work, and heard from its
/// workspace's record that it was worked in then, which makes it the workspace last worked in.
pub(crate) fn worked_in(ui: &mut Ui, stack: &Stack, chat: &SessionId) {
    after_every_workspace(ui);
    stack.client().session_prompt(chat, "say:done", None).unwrap();
    let replied = |c: &Session| c.user_active_at_ms.is_some() && c.has_reply() && !c.status.in_turn();
    let caught_up = |ui: &mut Ui| {
        let view = ui.view();
        let Some(c) = view.records().session(chat).filter(|c| replied(c)) else { return false };
        view.records().workspace(&c.workspace_id).is_some_and(|w| w.user_active_at_ms == c.user_active_at_ms)
    };
    ui.wait("the reply", caught_up);
    let worked = ui.view().records().session(chat).unwrap().workspace_id.clone();
    let others: Vec<_> =
        ui.view().records().workspaces().iter().map(|w| w.id.clone()).filter(|id| *id != worked).collect();
    let (at, others) = (last_active(ui, &[worked])[0], last_active(ui, &others));
    assert!(others.iter().all(|&other| other < at), "it's the one last worked in, at {at}, after {others:?}");
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
