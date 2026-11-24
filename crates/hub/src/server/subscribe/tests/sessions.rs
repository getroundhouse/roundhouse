//! How full a session's agent's context is, which a client of version 4 reads on the session's record, at the hub's
//! level, and a client of the version that reads every observation reads as an observation at the session's level.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{ContextUsage, DiffComment, DiffSide, Observation, Observed, Rev, Session};

use super::given::{ok, start};
use crate::hub::tests::hub_in;
use crate::hub::{SessionContext, SessionKey};
use crate::server::tests::Connection;

/// What `s`'s agent says of its context, with `used_tokens` in it.
fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

/// What the hub keeps `s`'s observations by.
fn s() -> SessionKey {
    SessionKey { workspace_id: "w".into(), session_id: "s".into() }
}

/// Each copy of `s`'s record `client` was sent, listed or told.
fn sessions_sent(client: &Connection) -> Vec<Session> {
    let copies = client.sent.iter().flat_map(|frame| {
        if let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame {
            return records.sessions.clone();
        }
        let HubFrame::Notice { notice: HubNotice::Session(session), .. } = frame else { return Vec::new() };
        vec![session.clone()]
    });
    copies.collect()
}

/// How full `s`'s context is, by what `client` was sent of it as an observation, in a list or a notice, in order.
fn contexts_observed(client: &Connection) -> Vec<Option<u64>> {
    let observed = |observations: &[Observation]| -> Vec<Option<u64>> {
        observations
            .iter()
            .filter_map(|observation| {
                let Observation::SessionContext { context, .. } = observation else { return None };
                let Observed::AsOf { value, .. } = context else { return Some(None) };
                Some(Some(value.used_tokens))
            })
            .collect()
    };
    let copies = client.sent.iter().flat_map(|frame| {
        if let HubFrame::Listed { listed: Listed::Session(records), .. } = frame {
            return observed(&records.observations);
        }
        if let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame {
            return observed(&records.observations);
        }
        let HubFrame::Notice { notice: HubNotice::Observation(observation), .. } = frame else { return Vec::new() };
        observed(std::slice::from_ref(observation))
    });
    copies.collect()
}

/// A hub whose session's context isn't known lists the session to a client of version 4 with none, and to one of the
/// version that reads every observation with no observation of it at the session's level. Two reports from the
/// session's agent, which the store keeps as `crate::session` does, then send the first the record twice, filled, each under a newer revision, and no observation, and
/// the second the observation twice, at the session's level, and no copy of the record, which moved only for them.
#[tokio::test]
async fn a_sessions_context_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    let follow_s = || start(2, Follow::Session { id: "s".into(), from: None });
    ok(&mut before, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub), follow_s()] }).await;
    ok(&mut observing, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub), follow_s()] }).await;
    for client in [&mut before, &mut observing] {
        let both_listed =
            |sent: &[HubFrame]| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count() == 2;
        client.until("both lists", both_listed).await;
    }
    let listed = sessions_sent(&before);
    assert_eq!(listed.iter().map(|session| session.context.clone()).collect::<Vec<_>>(), [None]);
    assert_eq!(contexts_observed(&observing), [], "not known");

    for (used_tokens, read_at_ms) in [(60_000, 1_000), (70_000, 2_000)] {
        hub.observation_read::<SessionContext>(s(), context(used_tokens), read_at_ms).await.unwrap();
    }
    let marker = HubNotice::Comment(DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "after".into(),
        created_at_ms: 1,
        // A copy with no revision is always sent.
        rev: Rev::default(),
    });
    hub.notify(marker.clone());

    for client in [&mut before, &mut observing] {
        let at_the_marker = |sent: &[HubFrame]| {
            sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice, .. } if *notice == marker))
        };
        client.until("the marker", at_the_marker).await;
    }
    let copies = sessions_sent(&before);
    let told: Vec<Option<u64>> =
        copies.iter().map(|session| session.context.as_ref().map(|context| context.used_tokens)).collect();
    assert_eq!(told, [None, Some(60_000), Some(70_000)]);
    assert!(copies.iter().map(|session| session.rev).is_sorted(), "{copies:?}");
    assert!(copies[1].rev > copies[0].rev, "the revision moved");
    assert_eq!(contexts_observed(&before), [], "no observation to a client of version 4");
    assert_eq!(contexts_observed(&observing), [Some(60_000), Some(70_000)]);
    assert_eq!(sessions_sent(&observing).len(), 1, "no copy that moves only how full the context is");
}
