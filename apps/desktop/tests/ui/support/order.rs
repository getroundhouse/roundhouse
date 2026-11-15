//! Workspaces whose order changes as they're worked in, as the sidebar and Home list them: three to put in order, more
//! than the sidebar has room for, working in one, and the order their rows are drawn in.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Pixels, TestAppContext};
use protocol::model::{RepoId, Session, SessionId, SetupStatus, Workspace, WorkspaceId};

use super::{Stack, Ui};

/// More workspaces than the sidebar has room for in the window, which shows 15, and few enough that a test makes
/// them quickly.
pub(crate) const MORE_THAN_FIT: usize = 24;

/// Makes `count` workspaces in `repo`, one after another, and answers them in the order they were made, each newer than
/// the one before.
///
/// From one thread, rather than several at once, until #1322: `git worktree add` can fail while another of the
/// repository's worktrees is part way through being made, and nothing yet keeps two workers from making them at once.
pub(crate) fn made_one_at_a_time(stack: &Stack, repo: &RepoId, count: usize) -> Vec<WorkspaceId> {
    let client = stack.client();
    let (mut made, mut newest) = (Vec::new(), i64::MIN);
    for _ in 0..count {
        let workspace = client.workspace_create(repo, None, None, None).unwrap();
        // Making one takes far longer than the millisecond its time is in, so each is newer, which the order relies on.
        assert!(workspace.created_at_ms > newest, "made in the same millisecond as the one before");
        newest = workspace.created_at_ms;
        made.push(workspace.id);
    }
    made
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
    (ui, workspaces.try_into().unwrap(), chats.try_into().unwrap())
}

/// A window with [`MORE_THAN_FIT`] workspaces in one repository, each set up, and the workspaces in the order they
/// were made.
pub(crate) fn more_than_fit<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Vec<WorkspaceId>) {
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let made = made_one_at_a_time(stack, &repo.id, MORE_THAN_FIT);
    let mut ui = Ui::open(cx, stack);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == MORE_THAN_FIT);
    wait_out_setup(&mut ui);
    (ui, made)
}

/// Waits until every workspace's setup has ended. A workspace's mark moves while its setup is running, and the hub
/// starts it a moment after it says the workspace is ready, so until it has ended, a mark can still ask for a frame
/// after every frame for as long as motion is allowed, and nothing could be seen to come to rest.
pub(crate) fn wait_out_setup(ui: &mut Ui) {
    let ended = |w: &Workspace| matches!(w.setup, SetupStatus::Succeeded | SetupStatus::Failed { .. });
    ui.wait("every workspace's setup to end", |ui| ui.view().records().workspaces().iter().all(ended));
}

/// Sends `chat` its first prompt, as the person would, and waits until the app has heard its agent reply, so nothing
/// in the window is still at work, and heard from its workspace's record that it was worked in then.
pub(crate) fn worked_in(ui: &mut Ui, stack: &Stack, chat: &SessionId) {
    stack.client().session_prompt(chat, "say:done", None).unwrap();
    let replied = |c: &Session| c.user_active_at_ms.is_some() && c.has_reply() && !c.status.in_turn();
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
