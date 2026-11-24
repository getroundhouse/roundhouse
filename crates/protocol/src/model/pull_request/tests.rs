use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

/// Checks that are `passed`, `failed` and `pending`, in that order, read at 1.
fn checks(passed: u32, failed: u32, pending: u32) -> Read<Checks> {
    let check = |state: CheckState| ObservedCheck {
        name: format!("{state:?}"),
        source: "app:1".into(),
        url: None,
        state,
        required: true,
    };
    let states = [(CheckState::Passed, passed), (CheckState::Failed, failed), (CheckState::Pending, pending)];
    let checks = states.into_iter().flat_map(|(state, n)| (0..n).map(move |_| check(state))).collect();
    let said = Checks { revision: Some("head-7".into()), requirements_known: true, checks };
    Read { said, read_at_ms: 1 }
}

/// An open pull request with three checks passed and nothing in the way, here and on GitHub.
fn open() -> OwnPullRequest {
    let pull_request = PullRequest { checks: Some(checks(3, 0, 0)), ..crate::fixtures::pull_request(7) };
    OwnPullRequest { pull_request, local: Local::default() }
}

fn standing(change: impl FnOnce(&mut OwnPullRequest)) -> Standing {
    let mut own = open();
    change(&mut own);
    Standing::of(Some(&own))
}

/// What GitHub says of it, to change.
fn said(own: &mut OwnPullRequest) -> &mut Summary {
    &mut own.pull_request.summary.said
}

#[test]
fn an_open_pull_request_with_nothing_in_the_way_is_ready() {
    assert_eq!(Standing::of(Some(&open())), Standing::Ready);
    assert_eq!(Standing::of(None), Standing::NoPullRequest);
}

/// A wait goes by GitHub alone, so the worktree's files and commits don't stand in its way, while merged, closed and
/// the queue still come first, as they do for the bar.
#[test]
fn on_github_alone_leaves_out_the_worktree() {
    let with = |change: fn(&mut Summary)| {
        let mut own = open();
        own.local = Local { uncommitted: 2, ahead: 1, behind: 0 };
        own.pull_request.checks = Some(checks(1, 0, 2));
        change(said(&mut own));
        Standing::on_github_alone(&own.pull_request)
    };
    let running = CheckCounts { passed: 1, failed: 0, pending: 2 };

    assert_eq!(with(|_| {}), Standing::ChecksRunning(running), "not the uncommitted files");
    assert_eq!(with(|pr| pr.queue = Some(QueueState::AwaitingChecks)), Standing::Queued(QueueState::AwaitingChecks));
    assert_eq!(with(|pr| pr.state = PrState::Merged), Standing::Merged);
    assert_eq!(with(|pr| pr.state = PrState::Closed), Standing::Closed);
}

#[test]
fn work_here_comes_before_what_github_says_about_an_older_commit() {
    let uncommitted = standing(|own| {
        own.local = Local { uncommitted: 3, ahead: 1, behind: 0 };
        said(own).merge_state = MergeState::Dirty;
        own.pull_request.checks = Some(checks(1, 2, 0));
    });
    let unpushed = standing(|own| {
        own.local.ahead = 2;
        said(own).merge_state = MergeState::Dirty;
    });

    assert_eq!(uncommitted, Standing::Uncommitted(3));
    assert_eq!(unpushed, Standing::Unpushed(2));
    assert_eq!(standing(|own| own.local.behind = 1), Standing::Behind(1));
    assert_eq!(standing(|own| own.local = Local { uncommitted: 0, ahead: 1, behind: 1 }), Standing::Diverged);
}

#[test]
fn what_github_can_merge_with_comes_after_what_it_cant() {
    let conflicts = standing(|own| {
        said(own).merge_state = MergeState::Dirty;
        said(own).review = Review::ChangesRequested;
    });
    let changes = standing(|own| {
        said(own).review = Review::ChangesRequested;
        said(own).is_draft = true;
    });
    let draft = standing(|own| {
        said(own).is_draft = true;
        own.pull_request.checks = Some(checks(1, 2, 1));
    });
    let failed = standing(|own| {
        own.pull_request.checks = Some(checks(1, 2, 1));
        said(own).review = Review::Required;
    });

    assert_eq!(conflicts, Standing::Conflicts);
    assert_eq!(changes, Standing::ChangesRequested);
    assert_eq!(draft, Standing::Draft);
    assert_eq!(failed, Standing::ChecksFailed(CheckCounts { passed: 1, failed: 2, pending: 1 }));
    let running = standing(|own| own.pull_request.checks = Some(checks(1, 0, 2)));
    assert_eq!(running, Standing::ChecksRunning(CheckCounts { passed: 1, failed: 0, pending: 2 }));
    assert_eq!(standing(|own| said(own).review = Review::Required), Standing::ReviewRequired);
    assert_eq!(standing(|own| said(own).merge_state = MergeState::Behind), Standing::Blocked);
    assert_eq!(standing(|own| said(own).merge_state = MergeState::Blocked), Standing::Blocked);
    assert_eq!(standing(|own| said(own).merge_state = MergeState::Computing), Standing::Checking);
}

/// Before its checks are read, as when only a chip's read has found it, what GitHub says of what its summary has
/// still comes first, and nothing after it is guessed at.
#[test]
fn checks_not_read_yet_stand_in_the_way_of_saying_more() {
    let unread = |change: fn(&mut OwnPullRequest)| {
        standing(|own| {
            own.pull_request.checks = None;
            change(own);
        })
    };

    assert_eq!(unread(|_| {}), Standing::Reading);
    assert_eq!(unread(|own| said(own).merge_state = MergeState::Dirty), Standing::Conflicts);
    assert_eq!(unread(|own| said(own).is_draft = true), Standing::Draft);
}

#[test]
fn what_this_version_doesnt_know_stands_as_unknown() {
    assert_eq!(standing(|own| said(own).state = PrState::Unknown), Standing::Unknown);
    assert_eq!(standing(|own| said(own).review = Review::Unknown), Standing::Unknown);
    assert_eq!(standing(|own| said(own).merge_state = MergeState::Unknown), Standing::Unknown);
    // What it does know about still comes first.
    let failing = standing(|own| {
        said(own).review = Review::Unknown;
        own.pull_request.checks = Some(checks(3, 1, 0));
    });
    assert_eq!(failing, Standing::ChecksFailed(CheckCounts { passed: 3, failed: 1, pending: 0 }));
}

/// GitHub's words for its state, merge state, review and place in the queue read by its own names, and words a newer
/// GitHub says as unknown.
#[test]
fn what_github_reports_decodes_by_its_own_names_and_anything_new_as_unknown() {
    let summary = |state: &str, merge_state: &str, review: &str, queue: &str| {
        let mut json = serde_json::to_value(crate::fixtures::pull_request(7).summary.said).unwrap();
        json["state"] = state.into();
        json["merge_state"] = merge_state.into();
        json["review"] = review.into();
        json["queue"] = queue.into();
        serde_json::from_value::<Summary>(json).unwrap()
    };

    let known = summary("OPEN", "UNKNOWN", "", "AWAITING_CHECKS");
    let newer = summary("REOPENED", "NEW", "NEW", "NEW");

    assert_eq!(
        (known.state, known.merge_state, known.review, known.queue),
        (PrState::Open, MergeState::Computing, Review::NotRequired, Some(QueueState::AwaitingChecks))
    );
    assert_eq!(
        (newer.state, newer.merge_state, newer.review, newer.queue),
        (PrState::Unknown, MergeState::Unknown, Review::Unknown, Some(QueueState::Unknown))
    );
}

/// A draft is a draft whatever's in the way of merging it, and one in the merge queue is queued, as a merged or
/// closed one is done whatever else is true.
#[test]
fn what_a_pull_request_is_comes_before_whats_in_its_way() {
    let phase = |change: &dyn Fn(&mut Summary)| {
        let mut own = open();
        change(said(&mut own));
        own.pull_request.phase()
    };
    let queued = Some(QueueState::Mergeable);

    assert_eq!(phase(&|_| {}), Phase::Open);
    assert_eq!(phase(&|pr| (pr.is_draft, pr.merge_state) = (true, MergeState::Dirty)), Phase::Draft);
    assert_eq!(phase(&|pr| pr.merge_state = MergeState::Draft), Phase::Draft);
    assert_eq!(phase(&|pr| (pr.is_draft, pr.queue) = (true, queued)), Phase::Queued(QueueState::Mergeable));
    assert_eq!(phase(&|pr| (pr.state, pr.queue) = (PrState::Merged, queued)), Phase::Merged);
    assert_eq!(phase(&|pr| pr.state = PrState::Closed), Phase::Closed);
    assert_eq!(phase(&|pr| pr.state = PrState::Unknown), Phase::Unknown);
}

fn any_pull_request() -> impl Strategy<Value = OwnPullRequest> {
    let state = prop_oneof![Just(PrState::Open), Just(PrState::Closed), Just(PrState::Merged), Just(PrState::Unknown)];
    let merge_state = prop_oneof![
        Just(MergeState::Behind),
        Just(MergeState::Blocked),
        Just(MergeState::Clean),
        Just(MergeState::Dirty),
        Just(MergeState::Draft),
        Just(MergeState::HasHooks),
        Just(MergeState::Unstable),
        Just(MergeState::Computing),
        Just(MergeState::Unknown),
    ];
    let review = prop_oneof![
        Just(Review::Approved),
        Just(Review::ChangesRequested),
        Just(Review::Required),
        Just(Review::NotRequired),
        Just(Review::Unknown)
    ];
    let queue = proptest::option::of(prop_oneof![Just(QueueState::AwaitingChecks), Just(QueueState::Mergeable)]);
    let counts = proptest::option::of((0..3u32, 0..3u32, 0..3u32));
    let local = (0..3u32, 0..3u32, 0..3u32);
    (state, any::<bool>(), merge_state, review, counts, queue, local).prop_map(
        |(state, is_draft, merge_state, review, counts, queue, (uncommitted, ahead, behind))| {
            let mut own = open();
            *said(&mut own) =
                Summary { state, is_draft, review, queue, merge_state, ..own.pull_request.said().clone() };
            own.pull_request.checks = counts.map(|(passed, failed, pending)| checks(passed, failed, pending));
            own.local = Local { uncommitted, ahead, behind };
            own
        },
    )
}

/// Not looked, looked and found none, and found one each read back as themselves on the record's link, as the
/// whole pull request does.
#[test]
fn which_pull_request_the_hub_found_reads_back_as_it_was() {
    use crate::model::{GithubRepositoryName, IssueNumber, PullRequestLink};
    let link = PullRequestLink {
        repository: GithubRepositoryName::named("o/r"),
        number: IssueNumber::try_from(7).unwrap(),
        url: "https://github.com/o/r/pull/7".into(),
    };
    for linked in [None, Some(PullRequestLinked { link: None }), Some(PullRequestLinked { link: Some(link) })] {
        let workspace =
            crate::model::Workspace { pull_request_link: linked.clone(), ..crate::fixtures::workspace("w") };
        let written = serde_json::to_string(&workspace).unwrap();

        let read: crate::model::Workspace = serde_json::from_str(&written).unwrap();

        assert_eq!(read.pull_request_link, linked, "{written}");
    }
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// Ready is only ever said of a pull request that could merge as it is here.
    #[test]
    fn ready_means_nothing_is_in_the_way(own in any_pull_request()) {
        let (pr, said) = (&own.pull_request, own.pull_request.said());
        let counts = pr.checks.as_ref().map(|checks| checks.said.counts());
        let nothing_in_the_way = said.state == PrState::Open
            && said.queue.is_none()
            && own.local == Local::default()
            && !said.is_draft
            && counts.is_some_and(|counts| counts.failed == 0 && counts.pending == 0)
            && matches!(said.review, Review::Approved | Review::NotRequired)
            && matches!(said.merge_state, MergeState::Clean | MergeState::HasHooks | MergeState::Unstable);

        prop_assert_eq!(Standing::of(Some(&own)) == Standing::Ready, nothing_in_the_way);
    }

    /// A merged or closed pull request is done, whatever else is true of it.
    #[test]
    fn done_comes_first(own in any_pull_request()) {
        let standing = Standing::of(Some(&own));

        match own.pull_request.said().state {
            PrState::Merged => prop_assert_eq!(standing, Standing::Merged),
            PrState::Closed => prop_assert_eq!(standing, Standing::Closed),
            PrState::Open | PrState::Unknown => prop_assert!(!matches!(standing, Standing::Merged | Standing::Closed)),
        }
    }

    /// A pull request reads back as it was written, every facet and word in it.
    #[test]
    fn a_pull_request_reads_back_as_it_was(own in any_pull_request()) {
        let written = serde_json::to_string(&own).unwrap();

        let read: OwnPullRequest = serde_json::from_str(&written).unwrap();

        prop_assert_eq!(read, own);
    }
}
