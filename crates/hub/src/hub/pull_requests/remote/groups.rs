//! Overlapping scopes share one read. Disjoint scopes keep separate retries, so one failing PR cannot starve them.

use protocol::model::{GitHubRepository, PullRequestNumber, PullRequestSelection, Wait, WorkspaceId};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::hub::pull_requests) type GroupKey = (WorkspaceId, String, Vec<PullRequestNumber>);
pub(super) struct Group {
    pub(super) repository: GitHubRepository,
    pub(super) selection: PullRequestSelection,
}

pub(super) fn groups(waits: Vec<Wait>) -> BTreeMap<GroupKey, Group> {
    let mut groups: Vec<(WorkspaceId, GitHubRepository, BTreeSet<PullRequestNumber>)> = Vec::new();
    for wait in waits {
        let Some(watch) = wait.watch else { continue };
        let mut numbers: BTreeSet<_> = watch.requested.numbers().iter().copied().collect();
        numbers.extend(
            watch
                .snapshot
                .pull_requests
                .iter()
                .filter_map(|pr| PullRequestNumber::try_from(pr.pull_request.number).ok()),
        );
        // Revisit after each union: a wait can join two formerly independent groups through a shared dependency.
        let mut index = 0;
        while index < groups.len() {
            let (workspace, repository, existing) = &groups[index];
            if *workspace == wait.workspace_id
                && *repository == watch.snapshot.repository
                && !numbers.is_disjoint(existing)
            {
                numbers.extend(groups.swap_remove(index).2);
                index = 0;
            } else {
                index += 1;
            }
        }
        groups.push((wait.workspace_id, watch.snapshot.repository, numbers));
    }
    groups
        .into_iter()
        .filter_map(|(workspace, repository, numbers)| {
            let numbers: Vec<_> = numbers.into_iter().collect();
            let selection = PullRequestSelection::try_from(numbers.clone()).ok()?;
            Some(((workspace, repository.url.clone(), numbers), Group { repository, selection }))
        })
        .collect()
}
