use std::collections::HashMap;
use std::time::{Duration, Instant};

use protocol::model::{
    Checks, Local, MergeState, PrState, PullRequest, PullRequestFound, Rev, Review, SetupStatus, TitleSource,
    Workspace, WorkspaceId, WorkspaceStatus,
};

use super::{Asked, LATEST, SOONEST, Schedule, Went, due};

fn pull_request(state: PrState) -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state,
        is_draft: false,
        base: None,
        merge_state: MergeState::Clean,
        review: Review::default(),
        checks: Checks::default(),
        queue: None,
        local: Local::default(),
    }
}

fn workspace(id: &str, status: WorkspaceStatus, state: Option<PrState>) -> Workspace {
    Workspace {
        id: id.into(),
        repo_id: "r".into(),
        name: id.to_string(),
        branch: id.to_string(),
        base_ref: None,
        path: Some(format!("/w/{id}")),
        status,
        provider: "local".to_string(),
        instance_id: id.into(),
        created_at_ms: 1,
        archived_at_ms: None,
        user_active_at_ms: None,
        setup: SetupStatus::Succeeded,
        port_base: 55_000,
        rev: Rev::default(),
        title: None,
        title_source: TitleSource::FirstLine,
        pull_request: Some(PullRequestFound { pull_request: state.map(pull_request) }),
    }
}

/// Each workspace read at this long ago, waiting the soonest.
fn read(ago: &[(&str, Duration)], now: Instant) -> HashMap<WorkspaceId, Schedule> {
    ago.iter().map(|(id, ago)| (WorkspaceId::from(*id), Schedule::starting(now.checked_sub(*ago).unwrap()))).collect()
}

fn next(workspaces: &[Workspace], schedules: &HashMap<WorkspaceId, Schedule>, now: Instant) -> Option<String> {
    due(workspaces, schedules, now).map(|w| w.id.to_string())
}

/// After a restart, what's open on GitHub is the likeliest to have moved, then whatever has waited longest.
#[test]
fn one_not_read_yet_comes_first_with_an_open_pull_request_first_then_the_longest_due() {
    let now = Instant::now();
    let workspaces = [
        workspace("due", WorkspaceStatus::Ready, Some(PrState::Open)),
        workspace("due-longer", WorkspaceStatus::Ready, Some(PrState::Open)),
        workspace("none", WorkspaceStatus::Ready, None),
        workspace("open", WorkspaceStatus::Ready, Some(PrState::Open)),
    ];
    let mut schedules = read(&[("due", SOONEST), ("due-longer", SOONEST * 2)], now);

    let first = next(&workspaces, &schedules, now);
    schedules.insert("open".into(), Schedule::starting(now));
    let second = next(&workspaces, &schedules, now);
    schedules.insert("none".into(), Schedule::starting(now));
    let third = next(&workspaces, &schedules, now);

    assert_eq!(first.as_deref(), Some("open"));
    assert_eq!(second.as_deref(), Some("none"));
    assert_eq!(third.as_deref(), Some("due-longer"));
}

#[test]
fn nothing_is_read_before_its_wait_is_up() {
    let now = Instant::now();
    let workspaces = [workspace("a", WorkspaceStatus::Ready, None)];
    let just_under = SOONEST.checked_sub(Duration::from_secs(1)).unwrap();

    assert_eq!(next(&workspaces, &read(&[("a", just_under)], now), now), None);
    assert_eq!(next(&workspaces, &read(&[("a", SOONEST)], now), now).as_deref(), Some("a"));
}

/// Merged and closed are done, and only a ready workspace has a worktree to read in.
#[test]
fn only_a_ready_workspace_whose_pull_request_can_still_change_is_read() {
    let workspaces = [
        workspace("merged", WorkspaceStatus::Ready, Some(PrState::Merged)),
        workspace("closed", WorkspaceStatus::Ready, Some(PrState::Closed)),
        workspace("archived", WorkspaceStatus::Archived, Some(PrState::Open)),
        workspace("creating", WorkspaceStatus::Creating, None),
    ];

    assert_eq!(next(&workspaces, &HashMap::new(), Instant::now()), None);
    let none = [workspace("none", WorkspaceStatus::Ready, None)];
    assert_eq!(next(&none, &HashMap::new(), Instant::now()).as_deref(), Some("none"));
}

/// So two hundred quiet workspaces cost little, and one that moves is back to the soonest.
#[test]
fn the_wait_doubles_while_nothing_changes_and_is_the_soonest_again_once_something_does() {
    let now = Instant::now();
    let quiet = |last| Schedule::after(Some(last), now, Asked::InTurn, Went::Same(None));

    let first = Schedule::after(None, now, Asked::InTurn, Went::Same(None));
    let waits: Vec<_> =
        std::iter::successors(Some(first), |last| Some(quiet(*last))).take(6).map(|s| s.every).collect();
    let changed = Schedule::after(Some(quiet(first)), now, Asked::InTurn, Went::Changed);
    let shown = Schedule::after(Some(quiet(first)), now, Asked::Waiting, Went::Same(None));

    assert_eq!(waits, [SOONEST * 2, SOONEST * 4, SOONEST * 8, LATEST, LATEST, LATEST]);
    assert_eq!(changed.every, SOONEST);
    assert_eq!(shown.every, SOONEST, "someone wanted to see it");
}

/// Checks running, the merge queue, and GitHub working out whether it can merge each finish by themselves.
#[test]
fn a_pull_request_still_moving_on_github_is_read_at_the_soonest_even_when_unchanged() {
    let now = Instant::now();
    let long = Schedule::after(
        Some(Schedule::after(None, now, Asked::InTurn, Went::Same(None))),
        now,
        Asked::InTurn,
        Went::Same(None),
    );
    let running = PullRequest { checks: Checks { passed: 1, failed: 0, pending: 2 }, ..pull_request(PrState::Open) };
    let settled = pull_request(PrState::Open);

    assert_eq!(Schedule::after(Some(long), now, Asked::InTurn, Went::Same(Some(&running))).every, SOONEST);
    assert_eq!(Schedule::after(Some(long), now, Asked::InTurn, Went::Same(Some(&settled))).every, SOONEST * 8);
}

/// Such as while GitHub can't be reached, when trying every workspace at the soonest would only fail as often.
#[test]
fn a_read_that_fails_waits_longer_each_time_however_it_was_asked_for() {
    let now = Instant::now();
    let first = Schedule::after(None, now, Asked::InTurn, Went::Failed);

    assert_eq!(first.every, SOONEST * 2);
    assert_eq!(Schedule::after(Some(first), now, Asked::InTurn, Went::Failed).every, SOONEST * 4);
    assert_eq!(Schedule::after(Some(first), now, Asked::Waiting, Went::Failed).every, SOONEST * 4);
}
