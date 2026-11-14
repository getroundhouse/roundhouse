use super::*;
use protocol::fixtures::{observed_pull_request, pull_request_wait};
use protocol::model::Wait;

fn wait(id: &str, numbers: &[u64]) -> Wait {
    let mut wait = pull_request_wait(id);
    let watch = wait.watch.as_mut().unwrap();
    watch.requested = PullRequestSelection::try_from(
        numbers.iter().map(|n| PullRequestNumber::try_from(*n).unwrap()).collect::<Vec<_>>(),
    )
    .unwrap();
    watch.snapshot.pull_requests = numbers.iter().map(|n| observed_pull_request(*n)).collect();
    wait
}

#[test]
fn overlapping_scopes_coalesce_transitively_and_unrelated_waits_have_separate_backoff() {
    let groups = groups(vec![wait("a", &[1, 2]), wait("b", &[3]), wait("c", &[2, 3]), wait("d", &[9])]);
    assert_eq!(groups.len(), 2);
    let now = Instant::now();
    let mut schedules = HashMap::new();
    let failing = groups.keys().find(|key| key.2.len() == 3).unwrap().clone();
    schedules.insert(failing, RemoteSchedule { next: now + BACKOFF_MAX, failures: 4 });

    let (_, group) = oldest_due(groups, &schedules, now).unwrap();

    assert_eq!(group.selection.numbers(), [PullRequestNumber::try_from(9).unwrap()]);
}

#[test]
fn the_oldest_due_group_precedes_an_earlier_repository_key() {
    let groups = groups(vec![wait("a", &[1]), wait("b", &[9])]);
    let now = Instant::now();
    let schedules = groups
        .keys()
        .map(|key| {
            let overdue = if key.2[0].get() == 9 { Duration::from_secs(30) } else { Duration::from_secs(1) };
            (key.clone(), RemoteSchedule { next: now.checked_sub(overdue).unwrap(), failures: 0 })
        })
        .collect();

    let (_, group) = oldest_due(groups, &schedules, now).unwrap();

    assert_eq!(group.selection.numbers(), [PullRequestNumber::try_from(9).unwrap()]);
}
