//! What the hub's observations of GitHub say, in the form a read of GitHub has, [`ReferenceRead`] and
//! [`OwnPullRequest`], for the views that read those. An observation holds no time of its own past when it was read, so
//! every part of what's made from one is as read at that one time.

use protocol::model::{
    Issue, IssueSaid, OwnPullRequest, OwnSummary, PullRequest, PullRequestDetail, PullRequestSaid, Read, Reference,
    ReferenceRead, ReferenceSaid,
};

/// What a chip's observation says, as read at `read_at_ms`. `None` for one of a kind this build doesn't know.
pub(super) fn reference(said: &ReferenceSaid, read_at_ms: i64) -> Option<ReferenceRead> {
    let reference = match said {
        ReferenceSaid::Issue(IssueSaid { repository, number, summary }) => Reference::Issue(Issue {
            repository: repository.clone(),
            number: *number,
            summary: Read { said: summary.clone(), read_at_ms },
        }),
        ReferenceSaid::PullRequest(said) => Reference::PullRequest(pull_request(said, read_at_ms)),
        ReferenceSaid::Missing => return Some(ReferenceRead::Missing),
        ReferenceSaid::Unknown => return None,
    };
    Some(ReferenceRead::Found { reference: Box::new(reference) })
}

/// A workspace's own pull request, read at `read_at_ms`, with `detail`'s checks, history and stack where they're of it
/// at the head its summary is at, and otherwise with none of them, since they're of another.
pub(super) fn own_pull_request(
    own: &OwnSummary,
    detail: Option<&PullRequestDetail>,
    read_at_ms: i64,
) -> OwnPullRequest {
    let said = &own.pull_request;
    let mut pull_request = pull_request(said, read_at_ms);
    let of_it = detail.filter(|detail| {
        detail.repository == said.repository && detail.number == said.number && detail.head == said.summary.head
    });
    if let Some(detail) = of_it {
        pull_request.checks = detail.checks.clone().map(|said| Read { said, read_at_ms });
        pull_request.history = detail.history.clone().map(|said| Read { said, read_at_ms });
        pull_request.stack = detail.stack.clone().map(|said| Read { said, read_at_ms });
    }
    OwnPullRequest { pull_request, local: own.local }
}

/// A pull request with its summary alone, read at `read_at_ms`.
fn pull_request(said: &PullRequestSaid, read_at_ms: i64) -> PullRequest {
    PullRequest {
        repository: said.repository.clone(),
        number: said.number,
        summary: Read { said: said.summary.clone(), read_at_ms },
        history: None,
        stack: None,
        checks: None,
    }
}
