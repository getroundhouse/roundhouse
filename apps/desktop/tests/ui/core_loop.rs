//! Starting out, and the work a session is for.

#![cfg(test)]

use gpui_kit::component::Theme;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{SessionStatus, WorkspaceId, WorkspaceStatus};

use crate::support::{Stack, Ui, git};

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

    let ui = Ui::open(cx, &stack);

    let label = ui.view().build_label();
    assert_eq!(label.text, protocol::BUILD_VERSION);
    assert!(!label.differs);
}

fn review_and_comment(ui: &mut Ui<'_>, stack: &Stack, id: &WorkspaceId) {
    // Review the diff.
    ui.click("toggle-diff");
    let pane = ui.pane().unwrap();
    ui.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
    let diff = ui.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
    ui.wait("the diff to load", |ui| ui.cx.read_entity(&diff, |d, _| d.state.is_some()));

    // Comment on a line. Enter saves it, and the hub keeps it.
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            diff.update(cx, |d, cx| {
                d.start_comment(DiffLinePosition::new("a.txt", DiffSide::Modified, 2), window, cx);
            });
        })
        .unwrap();
    ui.frame();
    ui.input("Why this word?");
    ui.press("enter");
    ui.wait("the comment to be saved", |ui| ui.cx.read_entity(&diff, |d, _| d.comments.len() == 1));
    assert_eq!(stack.client().comment_list(id).unwrap().len(), 1);

    // Send it to the agent, which sees where it was left.
    ui.click("send-comments");
    ui.wait("the comment to reach the agent", |ui| {
        ui.events().iter().any(
            |e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("`a.txt:2`: Why this word?")),
        )
    });
    ui.wait("the comment to be cleared", |ui| {
        ui.cx.read_entity(&diff, |d, _| d.comments.is_empty()) && stack.client().comment_list(id).unwrap().is_empty()
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

    // Work with the agent, approving a tool on the way.
    ui.input("say:starting; tool:echo changed >> a.txt; say:finished");
    ui.press("enter");
    let approval_seq = ui.next_open_approval();
    ui.wait("the session to wait for an answer", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's approved");
    ui.click(ElementId::NamedInteger("allow".into(), approval_seq));
    ui.wait_idle_with("the turn to finish", |text| text.contains("finished"));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");

    review_and_comment(&mut ui, &stack, &id);

    // Open a PR, which is the agent's job.
    ui.click("create-pr");
    ui.wait("the PR request to reach the agent", |ui| {
        ui.events()
            .iter()
            .any(|e| matches!(&e.body, EventBody::UserPrompt { text, .. } if text.contains("gh pr create")))
    });
    ui.wait("the turn to finish", |ui| ui.session_status() == Some(SessionStatus::Idle));

    // Archive. Uncommitted work means asking first, and "keep working" leaves it all alone.
    ui.click("archive");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();
    assert!(worktree.join("a.txt").exists());
    assert_eq!(ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status, WorkspaceStatus::Ready);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    ui.click("archive");
    ui.wait("the workspace to be archived", |ui| {
        ui.view().workspaces.iter().find(|w| w.id == id).unwrap().status == WorkspaceStatus::Archived && !ui.view().busy
    });
    assert!(!worktree.exists());
    assert!(ui.pane().is_none());
    assert!(!git(&stack.repo(), &["branch", "--list", &branch]).is_empty(), "the branch is kept");

    // Restoring brings the worktree and the history back, and the conversation continues.
    ui.click("toggle-archived");
    ui.cx.update_entity(&ui.view.clone(), |v, cx| v.restore(id.clone(), cx));
    ui.wait("the workspace to come back", |ui| ui.transcript().is_some() && !ui.view().busy);
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
    ui.press("cmd-shift-d");
    let pane = ui.pane().unwrap();
    ui.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff_open && p.diff.is_some()));
    let diff = ui.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
    ui.wait("it to see there are no changes", |ui| ui.cx.read_entity(&diff, |d, _| d.empty));

    let before = ui.view().workspaces.len();
    ui.press("cmd-n");
    ui.wait("a third workspace", |ui| ui.view().workspaces.len() == before + 1 && !ui.view().busy);
}

#[gpui_kit::test]
fn new_shows_its_working_until_the_workspace_is_ready_or_fails(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let repo = ui.first_repo_id();

    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.new_workspace(repo.clone(), cx));
    assert_eq!(ui.view().creating_in.as_ref(), Some(&repo));
    ui.wait("the workspace", |ui| ui.transcript().is_some() && ui.view().creating_in.is_none());

    // It can't get stuck showing that if creating one fails.
    std::fs::remove_dir_all(stack.repo()).unwrap();
    ui.cx.update_entity(&view, |v, cx| v.new_workspace(repo.clone(), cx));
    assert_eq!(ui.view().creating_in.as_ref(), Some(&repo));
    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().creating_in.is_none());
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
