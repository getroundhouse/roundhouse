//! The pull request's bar, from a stand-in for GitHub, since GitHub can't be reached from a test.

#![cfg(test)]

use client_core::ClientError;
use desktop::pull_request::Lookup;
use gpui_kit::{AppContext as _, TestAppContext};
use look::Work;
use protocol::client::ErrorCode;
use protocol::model::{PrState, WorkspaceId, WorkspaceStatus};

use hub::GithubPullRequestState::{Merged, Open};

use crate::support::{Stack, Ui};

/// What `read` makes of the pull request as the pane showing has it.
fn lookup<T>(ui: &Ui<'_>, read: impl FnOnce(&Lookup<'_>) -> T) -> T {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| read(&p.pull_request()))
}

fn state(ui: &Ui<'_>) -> Option<PrState> {
    lookup(ui, |lookup| lookup.pull_request().map(|pr| pr.summary.state))
}

/// The hub reads the pull request showing as often as the app shows it, so the app asks nothing itself, and shows
/// what each read finds, whatever made it, as the hub sends it.
#[gpui_kit::test]
fn the_pull_request_showing_is_what_the_hub_last_read(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();
    let (branch, _) = ui.push(&id);
    ui.wait("it to have no pull request", |ui| lookup(ui, |lookup| *lookup == Lookup::Found(None)));

    github.pull_requests.set_from(7, Open, &branch);
    stack.client().workspace_pull_request(&id).unwrap();
    ui.wait("the open pull request", |ui| state(ui) == Some(PrState::Open));

    github.pull_requests.set_from(7, Merged, &branch);
    stack.client().workspace_pull_request(&id).unwrap();
    ui.wait("it to merge", |ui| state(ui) == Some(PrState::Merged));
}

/// GitHub showing the default no repository says nothing of whether the branch has a pull request, so the hub knows
/// none, and the bar says it can't see it, rather than offer to create one. Its action opens Git in Settings, where an
/// account that can see it is connected, and so does the warning in the workspace's card.
#[gpui_kit::test]
fn a_repository_github_doesnt_show_has_a_bar_that_says_so_and_opens_git(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    github.pull_requests.hide(true);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();
    let branch = ui.push_unseen(&id);
    github.pull_requests.set_from(7, Open, &branch);

    let unread = stack.client().workspace_pull_request(&id).unwrap_err();

    let code = match unread {
        ClientError::Api(e) => Some(e.code),
        _ => None,
    };
    assert_eq!(code, Some(ErrorCode::Unavailable), "not the none known before the branch was pushed");
    lookup(&ui, |lookup| assert!(matches!(lookup, Lookup::Unseen(_)), "{lookup:?}"));
    assert!(ui.shows_label("pull-request-bar", "Open Settings"), "and offers no Create PR");
    ui.click("pull-request-action");
    ui.wait("Git in Settings", |ui| ui.has("git-connect"));
    ui.click("settings-back");
    ui.hover("workspace-title");
    ui.wait_out_a_tooltip();
    ui.click("warning-open-git");
    ui.wait("Git in Settings from the card", |ui| ui.has("git-connect"));
}

#[gpui_kit::test]
fn a_merged_pull_request_offers_archive_in_its_bar_and_in_the_header_while_the_bar_is_hidden(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let id = ui.new_workspace();
    // Merged from the commit the branch is at, as one the agent pushed is.
    let (branch, head) = ui.push(&id);
    github.pull_requests.set_at(7, Merged, &branch, &head);
    stack.client().workspace_pull_request(&id).unwrap();
    // The header stands in for the bar until the diff loads, so this waits for the bar itself.
    ui.wait("the bar to offer Archive", |ui| ui.shows_label("pull-request-bar", "Archive"));
    assert_eq!(ui.labelled("Archive").len(), 1, "only the bar offers it, and the header has none of its own");

    ui.press("secondary-shift-b");
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

/// The hub keeps every workspace's pull request on its record, so a row shows it without its workspace being opened,
/// and, as after an update, what the hub found is there the moment the app connects, even with GitHub out of reach.
#[gpui_kit::test]
fn a_rows_pull_request_shows_without_it_being_opened_and_at_once_when_the_app_opens_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let first = ui.new_workspace();
    let second = stack.client().workspace_create(&ui.first_repo_id(), None, None, None).unwrap().id;
    ui.wait("its row", |ui| ui.view().records().workspaces().iter().any(|w| w.id == second));
    for (number, id) in [(7, &first), (8, &second)] {
        let (branch, _) = ui.push(id);
        github.pull_requests.set_from(number, Open, &branch);
    }

    // Another client asking has the hub read it, as the hub does by itself every so often.
    for id in [&first, &second] {
        stack.client().workspace_pull_request(id).unwrap();
    }

    let both = |ui: &mut Ui| [&first, &second].iter().all(|id| work_shown(ui, id) == Some(Work::Open));
    ui.wait("both rows to show it, the second unopened", both);
    assert!(!ui.view().panes.contains_key(&second), "the second was never opened");
    drop(ui);
    github.pull_requests.fail(true);

    let mut ui = Ui::open(cx, &stack);

    ui.wait("both rows to show it once the app opens again", both);
    // GitHub can't say, so the bar of the one showing has it from the record too, rather than looking for it.
    ui.wait("the bar to show it", |ui| state(ui) == Some(PrState::Open));
}
