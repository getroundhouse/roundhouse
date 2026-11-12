//! The pull request's bar, from a stand-in `gh`, since GitHub can't be reached from a test.

#![cfg(test)]

use desktop::pull_request::Lookup;
use gpui_kit::{AppContext as _, TestAppContext};
use look::Work;
use protocol::model::{PrState, WorkspaceId, WorkspaceStatus};

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
        ui.view().records().workspaces().iter().any(|w| w.id == id && w.status == WorkspaceStatus::Archived)
    });
}

fn work_shown(ui: &mut Ui<'_>, id: &WorkspaceId) -> Option<Work> {
    let view = ui.view.clone();
    ui.cx.update(|cx| view.read(cx).work_shown(id, cx))
}

/// The hub keeps every workspace's pull request on its record, so a row shows it without its workspace being opened.
#[gpui_kit::test]
fn a_row_shows_its_pull_request_without_its_workspace_being_opened(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let elsewhere = stack.client().workspace_create(&ui.first_repo_id(), None, None, None).unwrap().id;
    ui.wait("its row", |ui| ui.view().records().workspaces().iter().any(|w| w.id == elsewhere));

    gh.report(&pull_request("OPEN"));
    // Another client asking has the hub read it, as the hub does by itself every so often.
    stack.client().workspace_pull_request(&elsewhere).unwrap();

    ui.wait("its row to show the pull request", |ui| work_shown(ui, &elsewhere) == Some(Work::Open));
    assert!(!ui.view().panes.contains_key(&elsewhere), "it was never opened");
}

/// As after an update: what the hub found is there the moment the app connects, even with GitHub out of reach.
#[gpui_kit::test]
fn an_app_that_opens_again_shows_every_rows_pull_request_at_once(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (first, second) = (ui.new_workspace(), ui.new_workspace());
    gh.report(&pull_request("OPEN"));
    for id in [&first, &second] {
        stack.client().workspace_pull_request(id).unwrap();
    }
    ui.wait("both rows to show it", |ui| [&first, &second].iter().all(|id| work_shown(ui, id) == Some(Work::Open)));
    drop(ui);
    gh.go_offline();

    let mut ui = Ui::open(cx, &stack);

    ui.wait("both rows to show it", |ui| [&first, &second].iter().all(|id| work_shown(ui, id) == Some(Work::Open)));
    // `gh` can't say, so the bar of the one showing has it from the record too, rather than looking for it.
    ui.wait("the bar to show it", |ui| state(ui) == Some(PrState::Open));
}
