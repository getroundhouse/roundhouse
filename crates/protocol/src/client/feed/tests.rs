use crate::model::{
    DiffComment, DiffSide, HubSettings, NewWorkspaceDraft, Record, RecordKey, Repo, Rev, Subagent, Terminal,
};

use super::{HubNotice, RecordNotice};

fn subagent(rev: u64) -> Subagent {
    Subagent { rev: Rev::new(rev), ..crate::fixtures::subagent("c") }
}

/// A notice of each kind of record that's quick to make, with the key and revision it's of.
fn of_a_record() -> Vec<(HubNotice, RecordKey, Rev)> {
    let repo = Repo { id: "r".into(), name: "repo".into(), path: "/r".into(), created_at_ms: 0, rev: Rev::new(2) };
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
                keep_awake: true,
                allow_asked_in_full_access: Some(true),
                rev: Rev::new(4),
            }),
            RecordKey::HubSettings,
            Rev::new(4),
        ),
        (HubNotice::NewWorkspaceDraft(draft), RecordKey::NewWorkspaceDraft, Rev::new(5)),
        (
            HubNotice::Subagent(subagent(6)),
            RecordKey::Subagent { session_id: "s".into(), call: "c".into() },
            Rev::new(6),
        ),
        (HubNotice::Terminal(terminal), RecordKey::Terminal { id: "t".into() }, Rev::new(8)),
        (HubNotice::Removed { record: gone.clone(), rev: Rev::new(7) }, gone, Rev::new(7)),
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

#[test]
fn a_notice_that_is_not_of_one_record_is_handed_back() {
    for notice in [HubNotice::Resync, HubNotice::Unknown] {
        assert_eq!(RecordNotice::try_from(notice.clone()), Err(notice));
    }
}
