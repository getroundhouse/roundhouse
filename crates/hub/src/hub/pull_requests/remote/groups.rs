//! Overlapping scopes share one ask. Disjoint scopes are asked apart, so one failing PR cannot fail the others.

use protocol::model::{GitHubRepository, PullRequestNumber, PullRequestSelection, Wait, WorkspaceId};
use std::collections::{BTreeSet, HashMap};

/// The pull requests of one repository that some of a workspace's waits watch, every wait whose scope meets another's
/// together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::hub::pull_requests) struct Waited {
    /// The repository they're pinned to, as GitHub named it as they started.
    pub(in crate::hub::pull_requests) repository: GitHubRepository,
    pub(in crate::hub::pull_requests) selection: PullRequestSelection,
}

/// What each workspace's `waits` watch: what each asked for, and what it last saw under that in its stack.
pub(in crate::hub::pull_requests) fn waited(waits: Vec<Wait>) -> HashMap<WorkspaceId, Vec<Waited>> {
    let mut groups: HashMap<WorkspaceId, Vec<(GitHubRepository, BTreeSet<PullRequestNumber>)>> = HashMap::new();
    for wait in waits {
        let Some(watch) = wait.watch else { continue };
        let mut numbers: BTreeSet<_> = watch.requested.numbers().iter().copied().collect();
        numbers.extend(watch.snapshot.pull_requests.iter().map(|pr| pr.number));
        let theirs = groups.entry(wait.workspace_id).or_default();
        // Revisit after each union: a wait can join two formerly independent groups through a shared dependency.
        let mut index = 0;
        while index < theirs.len() {
            let (repository, existing) = &theirs[index];
            if *repository == watch.snapshot.repository && !numbers.is_disjoint(existing) {
                numbers.extend(theirs.swap_remove(index).1);
                index = 0;
            } else {
                index += 1;
            }
        }
        theirs.push((watch.snapshot.repository, numbers));
    }
    let waited = |(repository, numbers): (GitHubRepository, BTreeSet<_>)| {
        let selection = PullRequestSelection::try_from(numbers.into_iter().collect::<Vec<_>>()).ok()?;
        Some(Waited { repository, selection })
    };
    groups.into_iter().map(|(id, theirs)| (id, theirs.into_iter().filter_map(waited).collect())).collect()
}
