use crate::model::{
    DiffComment, DiffSide, Ending, FoundGithubLogin, GithubAccount, GithubReach, GithubSignIn, HubSettings, Level,
    LineCounts, NewWorkspaceDraft, Observation, ObservationKey, Observed, Record, RecordKey, ReplyHeard, Repo, Rev,
    SettledBy, Subagent, Terminal,
};

use super::{HubNotice, ObservationNotice, RecordNotice};

fn subagent(rev: u64) -> Subagent {
    Subagent { rev: Rev::new(rev), ..crate::fixtures::subagent("c") }
}

/// A notice of each kind of record that's quick to make, with the key and revision it's of.
fn of_a_record() -> Vec<(HubNotice, RecordKey, Rev)> {
    let repo = Repo { rev: Rev::new(2), ..crate::fixtures::repo("r") };
    let comment = DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "why".into(),
        created_at_ms: 0,
        rev: Rev::new(3),
    };
    let draft = NewWorkspaceDraft { rev: Rev::new(5), ..NewWorkspaceDraft::default() };
    let terminal = Terminal { id: "t".into(), workspace_id: "w".into(), opened_at_ms: 0, rev: Rev::new(8) };
    let gone = RecordKey::Session { id: "s".into() };
    vec![
        (HubNotice::Repo(repo), RecordKey::Repo { id: "r".into() }, Rev::new(2)),
        (HubNotice::Comment(comment), RecordKey::Comment { id: "c".into() }, Rev::new(3)),
        (
            HubNotice::HubSettings(HubSettings {
                account_defaults: None,
                keep_awake: true,
                allow_asked_in_full_access: Some(true),
                github: None,
                rev: Rev::new(4),
            }),
            RecordKey::HubSettings,
            Rev::new(4),
        ),
        (HubNotice::NewWorkspaceDraft(draft), RecordKey::NewWorkspaceDraft, Rev::new(5)),
        (
            HubNotice::Subagent(subagent(6)),
            RecordKey::Subagent { session_id: "s".into(), subagent: "c".into() },
            Rev::new(6),
        ),
        (HubNotice::Terminal(terminal), RecordKey::Terminal { id: "t".into() }, Rev::new(8)),
        (HubNotice::Removed { record: gone.clone(), rev: Rev::new(7) }, gone, Rev::new(7)),
        (
            HubNotice::GithubAccount(GithubAccount { rev: Rev::new(8), ..crate::fixtures::github_account("g") }),
            RecordKey::GithubAccount { id: "g".into() },
            Rev::new(8),
        ),
        (
            HubNotice::GithubSignIn(GithubSignIn { rev: Rev::new(9), ..crate::fixtures::github_sign_in("s") }),
            RecordKey::GithubSignIn { id: "s".into() },
            Rev::new(9),
        ),
        (
            HubNotice::FoundGithubLogin(FoundGithubLogin {
                rev: Rev::new(10),
                ..crate::fixtures::found_github_login("local")
            }),
            RecordKey::FoundGithubLogin { provider: "local".into() },
            Rev::new(10),
        ),
        (
            HubNotice::GithubReach(GithubReach { rev: Rev::new(11), ..crate::fixtures::github_reach("r") }),
            RecordKey::GithubReach { repo: "r".into() },
            Rev::new(11),
        ),
    ]
}

#[test]
fn a_notice_of_a_record_says_which_record_and_revision_and_goes_back_as_it_came() {
    for (notice, key, rev) in of_a_record() {
        let of = RecordNotice::try_from(notice.clone()).unwrap();

        assert_eq!((of.key(), of.rev()), (key, rev), "{notice:?}");
        assert_eq!(HubNotice::from(of), notice);
    }
}

#[test]
fn a_copy_is_the_record_it_carries() {
    let of = RecordNotice::try_from(HubNotice::Subagent(subagent(6))).unwrap();

    assert_eq!(of, RecordNotice::Copy(Record::Subagent(subagent(6))));
}

/// A notice of an observation and of its removal, with the key and revision each is of.
fn of_an_observation() -> Vec<(HubNotice, ObservationKey, Rev)> {
    let key = ObservationKey::WorkspaceLines { workspace_id: "w".into() };
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    let copy = Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) };
    vec![
        (HubNotice::Observation(copy), key.clone(), Rev::new(4)),
        (HubNotice::ObservationRemoved { observation: key.clone(), rev: Rev::new(5) }, key, Rev::new(5)),
    ]
}

#[test]
fn a_notice_of_an_observation_says_which_and_its_revision_and_goes_back_as_it_came() {
    for (notice, key, rev) in of_an_observation() {
        let of = ObservationNotice::try_from(notice.clone()).unwrap();

        assert_eq!((of.key(), of.rev()), (key, rev), "{notice:?}");
        assert_eq!(HubNotice::from(of), notice);
    }
}

#[test]
fn a_notice_that_is_not_of_one_record_is_handed_back() {
    let observations = of_an_observation().into_iter().map(|(notice, _, _)| notice);
    for notice in [HubNotice::Resync, HubNotice::Unknown].into_iter().chain(observations) {
        assert_eq!(RecordNotice::try_from(notice.clone()), Err(notice));
    }
}

#[test]
fn a_notice_that_is_not_of_one_observation_is_handed_back() {
    let removed = HubNotice::Removed { record: RecordKey::Session { id: "s".into() }, rev: Rev::new(7) };
    for notice in [HubNotice::Subagent(subagent(6)), removed, HubNotice::Resync, HubNotice::Unknown] {
        assert_eq!(ObservationNotice::try_from(notice.clone()), Err(notice));
    }
}

/// Every notice goes to one level, or to none, and no subscription is sent what goes to none.
#[test]
fn each_notice_is_sent_at_the_level_it_is_of() {
    let s = || Some(Level::Session { id: "s".into() });
    let heard = ReplyHeard {
        session_id: "s".into(),
        message: "m".into(),
        ending: Ending::ToTheEnd,
        settled_by: SettledBy::default(),
        rev: Rev::new(12),
    };
    let removed = |record| HubNotice::Removed { record, rev: Rev::new(13) };
    let of_one_session = [
        (HubNotice::Proposal(crate::fixtures::proposal("p")), s()),
        (HubNotice::ReplyHeard(heard), s()),
        (removed(RecordKey::Subagent { session_id: "s".into(), subagent: "c".into() }), s()),
        (removed(RecordKey::ReplyHeard { session_id: "s".into(), message: "m".into() }), s()),
        (removed(RecordKey::Proposal { id: "p".into() }), None),
        (HubNotice::Resync, None),
        (HubNotice::Unknown, None),
    ];
    let records = of_a_record().into_iter().map(|(notice, key, _)| {
        let level = if matches!(key, RecordKey::Subagent { .. }) { s() } else { Some(Level::Hub) };
        (notice, level)
    });
    let observations = of_an_observation().into_iter().map(|(notice, _, _)| (notice, Some(Level::Hub)));

    for (notice, level) in records.chain(observations).chain(of_one_session) {
        assert_eq!(notice.level(), level, "{notice:?}");
    }
}
