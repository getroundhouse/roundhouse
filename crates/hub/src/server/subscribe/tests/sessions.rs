//! How full a session's agent's context is, what the API's cache holds of it, and what its waits read, which a client
//! of version 4 reads on the session's record, at the hub's level, and a client of the version that reads every
//! observation reads as observations, at the session's level and the hub's.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{
    CacheTtl, ContextUsage, ConversationCache, DiffComment, DiffSide, Observation, Observed, PromptCache, Rev, Session,
    Wait, WaitRead,
};

use super::given::{ok, start};
use crate::hub::tests::hub_in;
use crate::hub::{SessionCache, SessionContext, SessionKey, WaitKey};
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

/// What the API's cache holds of `s`, by each observation of it `client` was told, in order.
fn caches_observed(client: &Connection) -> Vec<ConversationCache> {
    let told = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::Observation(Observation::SessionCache { cache, .. }), .. } = frame
        else {
            return None;
        };
        let Observed::AsOf { value, .. } = cache else { return None };
        Some(value.clone())
    });
    told.collect()
}

/// Why each read of `s`'s wait that `client` was sent, listed or told, failed, if it did.
fn read_errors_observed(client: &Connection) -> Vec<Option<String>> {
    let error = |observation: &Observation| {
        let Observation::WaitRead { read: Observed::AsOf { value, .. }, .. } = observation else { return None };
        Some(value.read_error.clone())
    };
    let told = client.sent.iter().flat_map(|frame| {
        if let HubFrame::Listed { listed: Listed::Session(records), .. } = frame {
            return records.observations.iter().filter_map(error).collect();
        }
        let HubFrame::Notice { notice: HubNotice::Observation(observation), .. } = frame else { return Vec::new() };
        error(observation).into_iter().collect()
    });
    told.collect()
}

/// Whether `sent` has a read of `s`'s wait that failed, as a notice.
fn failed_read_told(sent: &[HubFrame]) -> bool {
    sent.iter().any(|frame| {
        let HubFrame::Notice { notice: HubNotice::Observation(Observation::WaitRead { read, .. }), .. } = frame else {
            return false;
        };
        matches!(read, Observed::AsOf { value, .. } if value.read_error.is_some())
    })
}

/// A hub whose session's context isn't known lists the session to a client of version 4 with none, and to one of the
/// version that reads every observation with no observation of it at the session's level. Two reports from the
/// session's agent, which the store keeps as `crate::session` does, then send the first the record twice, filled,
/// each under a newer revision, and no observation, and the second the observation twice, at the session's level, and
/// no copy of the record, which moved only for them. What the API's cache holds, at the hub's level, goes the same
/// way, and so does what an open wait of the session's read, at its level.
#[tokio::test]
async fn a_sessions_context_cache_and_waits_reach_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let wait = Wait { call: None, ..protocol::fixtures::pull_request_wait("a") };
    hub.start_wait(wait.clone()).await.unwrap();
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::greeted(&hub, EVERY_OBSERVATION_FROM).await;
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
    let held = PromptCache { used_at_ms: 3_000, ttl: CacheTtl::OneHour, model: None, tokens: 70_000 };
    let cache = ConversationCache { held: Some(held.clone()), worth_telling: true };
    hub.observation_worked_out::<SessionCache>(s(), cache.clone(), 3_000).await.unwrap();
    let read = WaitRead { read_error: Some("offline".into()), ..wait.watch.as_ref().unwrap().read() };
    let of = WaitKey { workspace_id: "w".into(), session_id: "s".into(), wait: "a".into() };
    hub.observation_worked_out::<crate::hub::WaitRead>(of, read, 1).await.unwrap();
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
    // The marker is at the hub's level, and each subscription is sent its notices by itself. So what's told at the
    // session's level can come after it, and its last, the wait's read, is waited for too.
    observing.until("the wait's read, at the session's level", failed_read_told).await;
    let copies = sessions_sent(&before);
    let told: Vec<Option<u64>> =
        copies.iter().map(|session| session.context.as_ref().map(|context| context.used_tokens)).collect();
    assert_eq!(told, [None, Some(60_000), Some(70_000), Some(70_000), Some(70_000)]);
    assert_eq!(copies.last().unwrap().prompt_cache, Some(held));
    let failed = copies.last().unwrap().open_waits()[0].watch.as_ref().unwrap().read_error.clone();
    assert_eq!(failed.as_deref(), Some("offline"), "the wait's watch filled");
    assert!(copies.windows(2).all(|pair| pair[0].rev < pair[1].rev), "the revision moved: {copies:?}");
    assert_eq!(contexts_observed(&before), [], "no observation to a client of version 4");
    assert_eq!(contexts_observed(&observing), [Some(60_000), Some(70_000)]);
    assert_eq!(caches_observed(&before), [], "no observation to a client of version 4");
    assert_eq!(caches_observed(&observing), [cache], "at the hub's level");
    assert_eq!(read_errors_observed(&before), [] as [Option<String>; 0], "no observation to a client of version 4");
    assert_eq!(read_errors_observed(&observing), [None, Some("offline".to_string())], "listed, then told");
    assert_eq!(sessions_sent(&observing).len(), 1, "no copy that moves only those");
}
