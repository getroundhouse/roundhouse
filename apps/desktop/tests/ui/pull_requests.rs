//! The pull request's bar, from a stand-in `gh`, since GitHub can't be reached from a test.

#![cfg(test)]

use desktop::pull_request::Lookup;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::PrState;

use crate::support::{Stack, Ui};

fn pull_request(state: &str) -> String {
    format!(r#"{{"number":7,"url":"https://example.com/pull/7","title":"T","state":"{state}","isDraft":false}}"#)
}

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
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
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
