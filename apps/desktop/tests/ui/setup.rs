//! A workspace's setup script.

#![cfg(test)]

use std::process::Command;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::SetupStatus;

use crate::support::{Stack, Ui, commit_settings};

#[gpui_kit::test]
fn a_failed_setup_is_shown_with_its_log_and_can_be_run_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Fails until the file it's looking for exists.
    let flag = stack.dir.path().join("may-succeed");
    commit_settings(&stack, &format!("[scripts]\nsetup = \"echo from-the-script; test -f {}\"\n", flag.display()));
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();
    let setup = |ui: &Ui| ui.cx.read_entity(&pane, |p, _| p.workspace.setup);
    ui.wait("setup to fail", |ui| matches!(setup(ui), SetupStatus::Failed { .. }));

    ui.click("setup-log");
    ui.wait("the log", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.setup_log.as_deref().is_some_and(|log| log.contains("from-the-script")))
    });
    ui.click("setup-log");
    assert!(ui.cx.read_entity(&pane, |p, _| p.setup_log.is_none()));

    std::fs::write(&flag, "").unwrap();
    ui.click("rerun-setup");
    ui.wait("setup to succeed", |ui| setup(ui) == SetupStatus::Succeeded);
    // Nothing more to say, so the line and its buttons go.
    ui.frame();
    let gone = ui.cx.update_window(ui.handle, |_, window, _| window.try_find("rerun-setup").is_none()).unwrap();
    assert!(gone);
}

/// A hub that stops cleanly says first that it stopped the setup, so only a crash leaves it for the next hub.
#[gpui_kit::test]
fn a_setup_cut_off_by_the_hub_crashing_shows_as_failed_after_it_comes_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 60\"\n");
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();
    ui.wait("setup to start", |ui| ui.cx.read_entity(&pane, |p, _| p.workspace.setup == SetupStatus::Running));

    let pid = stack.client().hub_status().unwrap().pid;
    assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());

    ui.wait("the new hub to say setup didn't finish", |ui| {
        ui.cx.read_entity(&pane, |p, _| matches!(p.workspace.setup, SetupStatus::Failed { code: None }))
    });
}

#[gpui_kit::test]
fn setup_can_be_stopped_from_the_app(cx: &mut TestAppContext) {
    let stack = Stack::new();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 300\"\n");
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();
    ui.wait("setup to start", |ui| ui.cx.read_entity(&pane, |p, _| p.workspace.setup == SetupStatus::Running));
    ui.click("stop-setup");
    ui.wait("setup to stop", |ui| {
        ui.cx.read_entity(&pane, |p, _| matches!(p.workspace.setup, SetupStatus::Failed { code: None }))
    });
}
