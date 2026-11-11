//! Starting out, and the work a session is for.

#![cfg(test)]

use desktop::app::BuildLine;
use desktop::new_workspace::Request;
use gpui_kit::component::Theme;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{
    DEFAULT_EFFORT, DEFAULT_MODEL, PermissionMode, RepoId, SessionStatus, WorkspaceId, WorkspaceStatus,
};

use desktop::busy::Busy;
use desktop::workspace_pane::Showing;

use crate::support::{Stack, Ui, git, home_row};

#[gpui_kit::test]
fn starts_empty_and_shows_a_repository_added_from_elsewhere(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.view().repos.is_empty());
    assert!(ui.pane().is_none());

    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository to appear", |ui| ui.view().repos.len() == 1);
    assert_eq!(ui.view().repos[0].name, "repo");
}

#[gpui_kit::test]
fn says_which_build_is_running(cx: &mut TestAppContext) {
    let stack = Stack::new();

    let mut ui = Ui::open(cx, &stack);

    assert_eq!(ui.build_line(), BuildLine::Current { app: protocol::BUILD_VERSION.to_string() });
}

fn review_and_comment(ui: &mut Ui<'_>, stack: &Stack, id: &WorkspaceId) {
    // Review the diff, opening the changed file from the sidebar.
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");

    // Comment on a line. Enter saves it, and the hub keeps it.
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why this word?");
    ui.press("enter");
    ui.wait("the comment to be saved", |ui| ui.comments(&diff).len() == 1);
    assert_eq!(stack.client().comment_list(id).unwrap().len(), 1);

    // Send it to the agent, which sees where it was left, and shows the chat it went to.
    ui.click("send-comments");
    let pane = ui.pane().unwrap();
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.showing()), Showing::Chat);
    ui.wait("the comment to reach the agent", |ui| {
        ui.events().iter().any(
            |e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("`a.txt:2`: Why this word?")),
        )
    });
    ui.wait("the comment to be cleared", |ui| {
        ui.comments(&diff).is_empty() && stack.client().comment_list(id).unwrap().is_empty()
    });
    ui.wait("the turn to finish", |ui| ui.session_status() == Some(SessionStatus::Idle));
}

#[gpui_kit::test]
fn the_core_loop(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);

    // Create a workspace. The composer is ready to type in straight away.
    let id = ui.new_workspace();
    let worktree = ui.worktree(&id);
    assert!(worktree.join("a.txt").exists());
    let branch = git(&worktree, &["branch", "--show-current"]);

    // Work with the agent, which has full access, so its call runs without asking.
    ui.input("say:starting; tool:echo changed >> a.txt; say:finished");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("finished"));
    assert!(!ui.events().iter().any(|e| matches!(e.body, EventBody::ApprovalRequested { .. })));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");

    review_and_comment(&mut ui, &stack, &id);

    // Open a PR, which is the agent's job, once the app knows there isn't one.
    ui.wait("the offer to create a PR", |ui| ui.has("pull-request-action"));
    ui.click("pull-request-action");
    ui.wait("the PR request to reach the agent", |ui| {
        ui.events()
            .iter()
            .any(|e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("gh pr create")))
    });
    ui.wait("the turn to finish", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Archive. Uncommitted work means asking first, and "keep working" leaves it all alone.
    ui.press("cmd-shift-a");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();
    assert!(worktree.join("a.txt").exists());
    assert_eq!(ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status, WorkspaceStatus::Ready);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    ui.press("cmd-shift-a");
    ui.wait("the workspace to be archived", |ui| {
        ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status == WorkspaceStatus::Archived
            && ui.view().busy == Busy::Free
    });
    assert!(!worktree.exists());
    assert!(ui.pane().is_none());
    assert!(!git(&stack.repo(), &["branch", "--list", &branch]).is_empty(), "the branch is kept");

    // Only Home lists it now, and pressing its row there brings the worktree and the history back, and the
    // conversation continues.
    ui.click("home");
    ui.click("home-archived");
    ui.click_menu_item("Showing all");
    ui.click(home_row(&id));
    ui.wait("the workspace to come back", |ui| ui.transcript().is_some() && ui.view().busy == Busy::Free);
    ui.wait("the history to load", |ui| !ui.events().is_empty());
    assert!(worktree.join("a.txt").exists());
    ui.input("recall");
    ui.press("enter");
    ui.wait_idle_with("the resumed agent to answer", |text| text.contains("I have seen:"));
    assert!(ui.assistant_text().contains("say:starting"), "{}", ui.assistant_text());
}

#[gpui_kit::test]
fn clicking_a_workspace_switches_to_it_and_typing_goes_to_the_right_place(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let first = ui.new_workspace();
    let second = ui.new_workspace();
    assert_ne!(first, second);
    assert_eq!(ui.view().selected.as_ref(), Some(&second));

    ui.click(SharedString::from(format!("workspace-{first}")));
    ui.wait("the first workspace", |ui| ui.view().selected.as_ref() == Some(&first) && ui.pane().is_some());
    ui.input("say:in the first");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("in the first"));

    ui.click(SharedString::from(format!("workspace-{second}")));
    ui.wait("the second workspace", |ui| ui.view().selected.as_ref() == Some(&second));
    ui.frame();
    assert!(!ui.assistant_text().contains("in the first"), "the second workspace has its own transcript");
    ui.input("say:in the second");
    ui.click("send");
    ui.wait_idle_with("the answer", |text| text.contains("in the second"));
    assert!(!ui.assistant_text().contains("in the first"));
}

#[gpui_kit::test]
fn the_keyboard_moves_between_workspaces_and_opens_the_diff(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let first = ui.new_workspace();
    let second = ui.new_workspace();

    ui.press("cmd-[");
    ui.wait("the previous workspace", |ui| ui.view().selected.as_ref() == Some(&first));
    ui.press("cmd-]");
    ui.wait("the next workspace", |ui| ui.view().selected.as_ref() == Some(&second));
    ui.press("cmd-]");
    ui.wait("wrapping around", |ui| ui.view().selected.as_ref() == Some(&first));

    ui.wait("the pane", |ui| ui.pane().is_some());
    let pane = ui.pane().unwrap();
    ui.press("cmd-shift-d");
    assert!(!ui.changes_open(), "it starts open, so this hides it");
    ui.press("cmd-shift-d");
    ui.wait("the sidebar of changes", |ui| ui.changes_open() && ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
    let review = ui.cx.read_entity(&pane, |p, _| p.diff.as_ref().map(|d| d.review.clone())).unwrap();
    ui.wait("it to see there are no changes", |ui| ui.cx.read_entity(&review, |r, _| r.has_no_changes()));

    let before = ui.view().workspaces.len();
    ui.press("cmd-n");
    ui.wait("the new-workspace dialog", Ui::has_dialog);
    ui.press("enter");
    ui.wait("a third workspace", |ui| ui.view().workspaces.len() == before + 1 && ui.view().busy == Busy::Free);
}

#[gpui_kit::test]
fn new_shows_its_working_until_the_workspace_is_ready_or_fails(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let repo = ui.first_repo_id();

    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(nothing_typed(&repo), cx)).unwrap();
    assert_eq!(ui.view().busy.creating_in(), Some(&repo));
    ui.wait("the workspace", |ui| ui.transcript().is_some() && ui.view().busy.is_free());

    // It can't get stuck showing that if creating one fails.
    std::fs::remove_dir_all(stack.repo()).unwrap();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(nothing_typed(&repo), cx)).unwrap();
    assert_eq!(ui.view().busy.creating_in(), Some(&repo));
    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().busy.is_free());
}

/// What the new-workspace dialog asks for when Create is pressed with nothing in it.
fn nothing_typed(repo: &RepoId) -> Request {
    let (model, effort) = (DEFAULT_MODEL, DEFAULT_EFFORT);
    let permission_mode = PermissionMode::FullAccess;
    Request { repo: repo.clone(), prompt: String::new(), files: Vec::new(), model, effort, permission_mode }
}

#[gpui_kit::test]
fn theme_is_dark(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);
    assert!(ui.cx.update(|cx| Theme::global(cx).mode.is_dark()));
}

#[gpui_kit::test]
fn an_archive_that_force_cant_help_shows_the_error_rather_than_offering_force(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let id = ui.new_workspace();

    // Archived from elsewhere, so what the app asks for can no longer work.
    stack.client().workspace_archive(&id, false).unwrap();
    ui.cx.update_entity(&ui.view.clone(), |v, cx| v.archive(id.clone(), false, cx));
    ui.wait("the error", |ui| ui.view().error.is_some());
    assert!(ui.view().error.clone().unwrap().contains("couldn't archive"));
    assert!(!ui.cx.has_pending_prompt());
}
