//! The pull request's bar, from a stand-in `gh`, since GitHub can't be reached from a test.

#![cfg(test)]

use desktop::pull_request::Lookup;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{PrState, WorkspaceStatus};

use crate::support::{Stack, Ui, pull_request};

fn lookup(ui: &Ui<'_>) -> Lookup {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.pull_request.clone())
}

fn state(ui: &Ui<'_>) -> Option<PrState> {
    lookup(ui).pull_request().map(|pr| pr.state)
}

#[gpui_kit::test]
fn the_pull_request_showing_is_read_again_until_its_merged(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.wait("it to have no pull request", |ui| lookup(ui) == Lookup::Found(None));

    // One opened on GitHub shows once it's time to read it again, and not before.
    gh.report(&pull_request("OPEN"));
    ui.frame();
    assert_eq!(lookup(&ui), Lookup::Found(None), "not read again before its time");
    ui.wait_out_a_pull_request_look();
    ui.wait("the open pull request", |ui| state(ui) == Some(PrState::Open));

    gh.report(&pull_request("MERGED"));
    ui.wait_out_a_pull_request_look();
    ui.wait("it to merge", |ui| state(ui) == Some(PrState::Merged));

    // Merged is done, so it isn't read again.
    gh.report(&pull_request("OPEN"));
    ui.wait_out_a_pull_request_look();
    ui.wait_out_a_pull_request_look();
    assert_eq!(state(&ui), Some(PrState::Merged));
}

#[gpui_kit::test]
fn a_merged_pull_request_offers_archive_in_its_bar_and_in_the_header_while_the_bar_is_hidden(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    gh.report(&pull_request("MERGED"));
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();
    // The header stands in for the bar until the diff loads, so this waits for the bar itself.
    ui.wait("the bar to offer Archive", |ui| ui.shows_label("pull-request-bar", "Archive"));
    assert_eq!(ui.labelled("Archive").len(), 1, "only the bar offers it, and the header has none of its own");

    ui.press("cmd-shift-d");
    assert!(!ui.changes_open());
    assert!(ui.shows_label("pull-request-controls", "Archive"), "the header offers it in the bar's place");
    ui.click("pull-request-action");

    ui.wait("it to be archived", |ui| {
        ui.view().workspaces.iter().any(|w| w.id == id && w.status == WorkspaceStatus::Archived)
    });
}
