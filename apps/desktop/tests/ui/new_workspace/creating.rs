#![cfg(test)]

use crate::support::{FakeHub, Held, Stack, Ui, git, sidebar_row};
use client_core::fake_hub::World;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;
use protocol::prompt::Prompt;

pub(super) fn open_with_a_repo<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui
}

/// The text of each prompt the chat showing has been sent, in order.
pub(super) fn prompts(ui: &Ui<'_>) -> Vec<String> {
    let prompts = ui.events().into_iter().filter_map(|event| match event.body {
        EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } => Some(text),
        _ => None,
    });
    prompts.collect()
}

/// The dialog opens for the repository whose `+` was pressed, and makes the workspace in another once it's picked,
/// which the menu lists as the sidebar does. One made with nothing typed starts no agent. What the hub gives the
/// first chat, and that it sends nothing when nothing's typed, is the hub's rule, which its tests in `new_workspace`
/// pin.
#[gpui_kit::test]
fn what_was_typed_is_the_first_prompt_of_a_workspace_made_in_the_repository_picked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let other = stack.dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]);
    git(&other, &["commit", "-q", "--allow-empty", "-m", "first"]);
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    client.repo_add(other.to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("both repositories", |ui| ui.view().records().repos().len() == 2);
    let second = ui.view().records().repos()[1].id.clone();
    let repo_of = |ui: &Ui<'_>, workspace| {
        ui.view().records().workspaces().iter().find(|w| w.id == workspace).map(|w| w.repo_id.clone())
    };
    let before = ui.view().records().workspaces().len();

    ui.open_new_workspace();
    ui.input("say:on it");
    ui.press("enter");

    let workspace = ui.wait_for_a_new_workspace(before);
    assert!(!ui.has_dialog(), "Create closes the dialog");
    ui.wait_idle_with("the agent to answer the first prompt", |text| text == "on it");
    assert_eq!(prompts(&ui), ["say:on it"]);
    assert_eq!(repo_of(&ui, workspace), Some(ui.first_repo_id()), "made where the dialog opened for");

    ui.open_new_workspace();
    assert_eq!(ui.dialog_text(), "", "the next workspace starts with an empty dialog");
    ui.pick("new-workspace-repo", 1);
    let workspace = ui.create_workspace();

    assert_eq!(repo_of(&ui, workspace), Some(second), "made in the repository picked");
    assert!(prompts(&ui).is_empty(), "nothing typed, nothing sent");
    assert!(!ui.events().iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })), "no agent was started");
}

/// Files with these names, written to `dir`, copied as Finder copies them.
fn copied(dir: &std::path::Path, names: &[&str]) -> ClipboardItem {
    let paths = names.iter().map(|name| {
        let path = dir.join(name);
        std::fs::write(&path, "the first line\n").unwrap();
        path
    });
    ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(paths.collect())))
}

/// What's in the dialog is the app's until the workspace is made, which these don't do, so a fake hub will do, one
/// that keeps nothing of the dialog, as an older hub doesn't. Closing the dialog keeps what's in it for next time, and
/// asks nothing of the hub. A file pasted, or chosen from its `+` menu, waits in it. Plan mode turns on with Shift-Tab
/// or the `+` menu, as in the composer. A workspace that shows behind it, as one does once it's made, is made a pane
/// that brings its chat to the front, which would focus its composer, but the keys stay with the dialog.
#[gpui_kit::test]
fn the_dialog_keeps_whats_typed_attached_and_planned_and_the_keys_while_another_workspace_shows(
    cx: &mut TestAppContext,
) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspaces = ["ledger", "points"].map(|name| world.workspace(&repo, name));
    for workspace in &workspaces {
        world.chat(workspace, SessionStatus::Idle, Vec::new());
    }
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.transcript().is_some());
    let showing = ui.view().selected().cloned().unwrap();
    let other = workspaces.into_iter().find(|w| *w != showing).unwrap();
    let files = tempfile::tempdir().unwrap();

    ui.press("secondary-n");
    ui.wait("the dialog, from its key", Ui::has_dialog);
    ui.input("half a thought");
    ui.press("escape");

    assert!(!ui.has_dialog(), "Escape closes it");
    let view = ui.view();
    assert!(view.busy.is_free() && view.error.is_none(), "closing asks nothing of the hub, so makes nothing");
    ui.open_new_workspace();
    assert_eq!(ui.dialog_text(), "half a thought", "what was typed is there next time");

    ui.paste(copied(files.path(), &["notes.txt"]));
    ui.wait("the pasted file", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert_eq!(ui.dialog_text(), "half a thought", "a copied file isn't text");
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Attach files…");
    ui.wait("the file picker", |ui| ui.cx.did_prompt_for_paths());
    let chosen = files.path().join("plan.txt");
    std::fs::write(&chosen, "a plan\n").unwrap();
    ui.cx.simulate_path_prompt_response(|_| Some(vec![chosen.clone()]));
    ui.wait("the file chosen from the menu", |ui| ui.dialog_attached() == ["notes.txt", "plan.txt"]);

    ui.press("shift-tab");
    assert!(ui.dialog_planning(), "Shift-Tab turns plan mode on");
    assert!(ui.shows_label("new-workspace", "Plan mode"), "the dialog shows it");
    assert_eq!(ui.dialog_text(), "half a thought", "Shift-Tab types nothing");
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Plan mode");
    assert!(!ui.dialog_planning(), "the menu turns it off");
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Plan mode");
    assert!(ui.dialog_planning(), "and on again");

    ui.input(" next");
    // As a workspace being made is selected once it's ready.
    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.select(other.clone(), cx));
    ui.wait("the other workspace's pane", |ui| ui.transcript().is_some() && ui.view().selected() == Some(&other));
    ui.input(" task");

    assert_eq!(ui.dialog_text(), "half a thought next task", "the keys stayed with the dialog");
    assert_eq!(ui.composer_text(), "");
    ui.press("escape");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "closing the dialog focuses the workspace showing");
}

/// Where the person went while the new one waits on git, which is where making one takes its time, stays showing once
/// it's made: another workspace chosen from the sidebar, Settings, and Home. Each has the keys once it shows.
#[gpui_kit::test]
fn what_was_opened_while_a_workspace_is_made_stays_showing_once_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let first = ui.new_workspace();
    let second = ui.new_workspace();
    let making = Held::checkout(&stack);
    // Starts making one more, and waits until git holds it.
    let start_making = |ui: &mut Ui<'_>, runs: usize, typed: &str| {
        ui.open_new_workspace();
        ui.input(typed);
        ui.press("enter");
        ui.wait(&format!("it to start making workspace {}", runs + 2), |_| making.runs() == runs);
    };
    let made = |ui: &Ui<'_>, count: usize| ui.view().records().workspaces().len() == count && ui.view().busy.is_free();

    start_making(&mut ui, 1, "say:on it");
    ui.click(sidebar_row(&first));
    making.release();

    ui.wait("the third workspace", |ui| made(ui, 3));
    assert_eq!(ui.view().selected(), Some(&first), "the one chosen meanwhile stays showing");
    let third =
        ui.view().records().workspaces().iter().map(|w| w.id.clone()).find(|id| ![&first, &second].contains(&id));
    let third = third.unwrap();
    let prompted = |ui: &mut Ui| ui.view().records().sessions(&third).iter().any(|c| c.user_active_at_ms.is_some());
    ui.wait("the third workspace's chat to have its prompt", prompted);

    making.hold_again();
    start_making(&mut ui, 2, "");
    ui.wait_out_the_dialog_going();
    ui.press("secondary-,");
    ui.wait("Settings", |ui| ui.has("settings-back"));
    making.release();

    ui.wait("the fourth workspace", |ui| made(ui, 4));
    assert_eq!(ui.view().selected(), Some(&first), "the one under Settings stays under it");
    ui.press("escape");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "and has the keys once Settings goes");

    making.hold_again();
    start_making(&mut ui, 3, "");
    ui.click("home");
    making.release();

    ui.wait("the fifth workspace", |ui| made(ui, 5));
    assert!(ui.has("home-page"), "Home stays showing");
    ui.input("a name no workspace has");
    ui.wait("Home's search to have the keys", |ui| ui.has("home-nothing"));
}
