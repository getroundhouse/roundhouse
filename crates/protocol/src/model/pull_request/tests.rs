use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

fn open() -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state: PrState::Open,
        is_draft: false,
        base: Some("main".to_string()),
        merge_state: MergeState::Clean,
        review: Review::NotRequired,
        checks: Checks { passed: 3, failed: 0, pending: 0 },
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    }
}

fn standing(change: impl FnOnce(&mut PullRequest)) -> Standing {
    let mut pr = open();
    change(&mut pr);
    Standing::of(Some(&pr))
}

#[test]
fn an_open_pull_request_with_nothing_in_the_way_is_ready() {
    assert_eq!(Standing::of(Some(&open())), Standing::Ready);
    assert_eq!(Standing::of(None), Standing::NoPullRequest);
}

#[test]
fn work_here_comes_before_what_github_says_about_an_older_commit() {
    let failing = Checks { passed: 1, failed: 2, pending: 0 };

    let uncommitted = standing(|pr| {
        pr.local = Local { uncommitted: 3, ahead: 1, behind: 0 };
        pr.merge_state = MergeState::Dirty;
        pr.checks = failing;
    });
    let unpushed = standing(|pr| {
        pr.local.ahead = 2;
        pr.merge_state = MergeState::Dirty;
    });

    assert_eq!(uncommitted, Standing::Uncommitted(3));
    assert_eq!(unpushed, Standing::Unpushed(2));
    assert_eq!(standing(|pr| pr.local.behind = 1), Standing::Behind(1));
    assert_eq!(standing(|pr| pr.local = Local { uncommitted: 0, ahead: 1, behind: 1 }), Standing::Diverged);
}

#[test]
fn what_github_can_merge_with_comes_after_what_it_cant() {
    let failing = Checks { passed: 1, failed: 2, pending: 1 };
    let running = Checks { passed: 1, failed: 0, pending: 2 };

    let conflicts = standing(|pr| {
        pr.merge_state = MergeState::Dirty;
        pr.review = Review::ChangesRequested;
    });
    let changes = standing(|pr| {
        pr.review = Review::ChangesRequested;
        pr.is_draft = true;
    });
    let draft = standing(|pr| {
        pr.is_draft = true;
        pr.checks = failing;
    });
    let failed = standing(|pr| {
        pr.checks = failing;
        pr.review = Review::Required;
    });

    assert_eq!(conflicts, Standing::Conflicts);
    assert_eq!(changes, Standing::ChangesRequested);
    assert_eq!(draft, Standing::Draft);
    assert_eq!(failed, Standing::ChecksFailed(failing));
    assert_eq!(standing(|pr| pr.checks = running), Standing::ChecksRunning(running));
    assert_eq!(standing(|pr| pr.review = Review::Required), Standing::ReviewRequired);
    assert_eq!(standing(|pr| pr.merge_state = MergeState::Behind), Standing::Blocked);
    assert_eq!(standing(|pr| pr.merge_state = MergeState::Blocked), Standing::Blocked);
    assert_eq!(standing(|pr| pr.merge_state = MergeState::Computing), Standing::Checking);
}

#[test]
fn what_this_version_doesnt_know_stands_as_unknown() {
    let older_hub = r#"{"number":7,"url":"u","title":"T","state":"OPEN","is_draft":false}"#;
    let older_hub: PullRequest = serde_json::from_str(older_hub).unwrap();

    assert_eq!(Standing::of(Some(&older_hub)), Standing::Unknown, "a hub that doesn't say how it can merge");
    assert_eq!(standing(|pr| pr.state = PrState::Unknown), Standing::Unknown);
    assert_eq!(standing(|pr| pr.review = Review::Unknown), Standing::Unknown);
    assert_eq!(standing(|pr| pr.merge_state = MergeState::Unknown), Standing::Unknown);
    // What it does know about still comes first.
    let failing = standing(|pr| {
        pr.review = Review::Unknown;
        pr.checks.failed = 1;
    });
    assert_eq!(failing, Standing::ChecksFailed(Checks { passed: 3, failed: 1, pending: 0 }));
}

#[test]
fn what_github_reports_decodes_by_its_own_names_and_anything_new_as_unknown() {
    let json = r#"{"number":7,"url":"u","title":"T","state":"OPEN","is_draft":false,"merge_state":"UNKNOWN",
        "review":"","queue":"AWAITING_CHECKS"}"#;
    let newer = r#"{"number":7,"url":"u","title":"T","state":"REOPENED","is_draft":false,"merge_state":"NEW",
        "review":"NEW","queue":"NEW"}"#;

    let pr: PullRequest = serde_json::from_str(json).unwrap();
    let newer: PullRequest = serde_json::from_str(newer).unwrap();

    assert_eq!(
        (pr.state, pr.merge_state, pr.review, pr.queue),
        (PrState::Open, MergeState::Computing, Review::NotRequired, Some(QueueState::AwaitingChecks))
    );
    assert_eq!(
        (newer.state, newer.merge_state, newer.review, newer.queue),
        (PrState::Unknown, MergeState::Unknown, Review::Unknown, Some(QueueState::Unknown))
    );
}

#[test]
fn every_word_reads_back_as_itself() {
    let merge_states = [
        MergeState::Behind,
        MergeState::Blocked,
        MergeState::Clean,
        MergeState::Dirty,
        MergeState::Draft,
        MergeState::HasHooks,
        MergeState::Unstable,
        MergeState::Computing,
        MergeState::Unknown,
    ];
    let reviews = [Review::Approved, Review::ChangesRequested, Review::Required, Review::NotRequired, Review::Unknown];

    for word in merge_states {
        let sent = serde_json::to_string(&word).unwrap();
        assert_eq!(serde_json::from_str::<MergeState>(&sent).unwrap(), word, "{sent}");
    }
    for word in reviews {
        let sent = serde_json::to_string(&word).unwrap();
        assert_eq!(serde_json::from_str::<Review>(&sent).unwrap(), word, "{sent}");
    }
}

fn any_pull_request() -> impl Strategy<Value = PullRequest> {
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
    let counts = (0..3u32, 0..3u32, 0..3u32);
    (state, any::<bool>(), merge_state, review, counts.clone(), queue, counts).prop_map(
        |(state, is_draft, merge_state, review, (passed, failed, pending), queue, (uncommitted, ahead, behind))| {
            PullRequest {
                state,
                is_draft,
                merge_state,
                review,
                checks: Checks { passed, failed, pending },
                queue,
                local: Local { uncommitted, ahead, behind },
                ..open()
            }
        },
    )
}

/// That there's none, or any pull request.
fn any_found() -> impl Strategy<Value = PullRequestFound> {
    proptest::option::of(any_pull_request()).prop_map(|pull_request| PullRequestFound { pull_request })
}

/// A workspace whose hub found this of its pull request.
fn workspace(pull_request: Option<PullRequestFound>) -> crate::model::Workspace {
    crate::model::Workspace {
        id: "w".into(),
        repo_id: "r".into(),
        name: "n".to_string(),
        branch: "n".to_string(),
        base_ref: None,
        path: None,
        status: crate::model::WorkspaceStatus::Ready,
        provider: "local".into(),
        instance_id: "w".into(),
        created_at_ms: 1,
        archived_at_ms: None,
        user_active_at_ms: None,
        setup: crate::model::SetupStatus::Succeeded,
        port_base: 55_000,
        rev: crate::model::Rev::default(),
        title: None,
        title_source: crate::model::TitleSource::FirstLine,
        pull_request,
        comment_draft: None,
        branches: None,
        pull_request_branch: None,
    }
}

/// One found on a branch other than the one checked out isn't the workspace's now, but none found on no branch, as
/// where `gh` can't work at all, says nothing of any branch.
#[test]
fn only_what_was_found_on_a_branch_can_be_another_branchs() {
    use crate::model::{BranchName, Branches, Head};
    let on = |pull_request: Option<PullRequest>, branch: Option<&str>| crate::model::Workspace {
        branches: Some(Branches { head: Head::Branch { name: BranchName::from("next") }, had: Vec::new() }),
        pull_request_branch: branch.map(BranchName::from),
        ..workspace(Some(PullRequestFound { pull_request }))
    };

    assert!(on(Some(open()), Some("aspen")).pull_request_found_on_another_branch());
    assert!(on(None, Some("aspen")).pull_request_found_on_another_branch(), "the branch on now may have one");
    assert!(on(Some(open()), None).pull_request_found_on_another_branch(), "found before branches were kept");
    assert!(!on(Some(open()), Some("next")).pull_request_found_on_another_branch());
    assert!(!on(None, None).pull_request_found_on_another_branch(), "as where gh can't work");
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// Not looked, looked and found none, and found one each read back as themselves.
    #[test]
    fn what_the_hub_found_reads_back_as_it_was(found in proptest::option::of(any_found())) {
        let written = serde_json::to_string(&workspace(found.clone())).unwrap();

        let read: crate::model::Workspace = serde_json::from_str(&written).unwrap();

        prop_assert_eq!(read.pull_request, found);
    }

    /// Ready is only ever said of a pull request that could merge as it is here.
    #[test]
    fn ready_means_nothing_is_in_the_way(pr in any_pull_request()) {
        let nothing_in_the_way = pr.state == PrState::Open
            && pr.queue.is_none()
            && pr.local == Local::default()
            && !pr.is_draft
            && pr.checks.failed == 0
            && pr.checks.pending == 0
            && matches!(pr.review, Review::Approved | Review::NotRequired)
            && matches!(pr.merge_state, MergeState::Clean | MergeState::HasHooks | MergeState::Unstable);

        prop_assert_eq!(Standing::of(Some(&pr)) == Standing::Ready, nothing_in_the_way);
    }

    /// A merged or closed pull request is done, whatever else is true of it.
    #[test]
    fn done_comes_first(pr in any_pull_request()) {
        let standing = Standing::of(Some(&pr));

        match pr.state {
            PrState::Merged => prop_assert_eq!(standing, Standing::Merged),
            PrState::Closed => prop_assert_eq!(standing, Standing::Closed),
            PrState::Open | PrState::Unknown => prop_assert!(!matches!(standing, Standing::Merged | Standing::Closed)),
        }
    }
}
