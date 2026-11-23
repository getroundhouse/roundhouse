//! What the API's cache holds of a chat's conversation, which the session tells of as the observation `SessionCache`,
//! and fills the chat's record with for a client of a version from before it, and which the next request is compared
//! with to find whether it found the cache expired.

use std::path::Path;
use std::sync::Arc;

use protocol::capability::{Capabilities, Claimed, Report, context};
use protocol::event::EventBody;
use protocol::model::{
    CacheRequest, CacheTtl, ContextUsage, ConversationCache, Observation, Observed, PromptCache, RunCapabilities,
    RunId, Seq, SessionStatus,
};
use rusqlite::Connection;

use super::super::{Item, LiveSession};
use crate::hub::Hub;
use crate::hub::tests::{carried_on, flushed, hub_in, hub_in_again, stand_in_worker, store_of};
use crate::store::BEFORE_SESSION_CACHE_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::store::tests::idle_session;

const RUN: RunId = RunId::new(1);

/// `s`, carried on with on `hub`, whose run claims to say how full its context is and what each request cached.
fn chat(hub: &Arc<Hub>) -> Arc<LiveSession> {
    let link = stand_in_worker(hub);
    let live = carried_on(hub, &link, RUN);
    let mut claims = Capabilities::default();
    claims.context.usage = Some(Claimed {});
    claims.context.cache = Some(context::Cache { kind: context::CacheKind::Marked { ttls: vec![CacheTtl::OneHour] } });
    live.state.lock().capabilities = Some(RunCapabilities::new(RUN, claims));
    live
}

/// A prompt's first request, which read `read_tokens` of the cache and wrote `written_tokens` to it.
fn request(read_tokens: u64, written_tokens: u64) -> CacheRequest {
    CacheRequest {
        sent_ms_ago: 0,
        ttl: Some(CacheTtl::OneHour),
        read_tokens,
        written_tokens,
        model: None,
        first_for_prompt: true,
    }
}

/// What `live`'s agent says, once the session has taken it in.
async fn says(live: &LiveSession, report: Report) {
    live.send_item(Item::Report { run: RUN, report, seq: None }).await;
    flushed(live).await;
}

/// How full a context with `used_tokens` in it is.
fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

/// What `hub` lists of what the cache holds of `s`, and what `s`'s record says of it, as a reply carries it.
async fn listed(hub: &Hub) -> (Option<Observed<ConversationCache>>, Option<PromptCache>) {
    let records = hub.records_list().await.unwrap();
    let observed = records.observations.into_iter().find_map(|observation| {
        let Observation::SessionCache { cache, .. } = observation else { return None };
        Some(cache)
    });
    let session = records.sessions.into_iter().find(|session| session.id.as_str() == "s").unwrap();
    (observed, session.prompt_cache)
}

/// Whether `s`'s log says a request found the cache expired.
fn found_expired(hub: &Hub) -> bool {
    let events = store_of(hub).read_events(&"s".into(), Seq::FIRST, 1000, u64::MAX, false).unwrap();
    events.iter().any(|event| matches!(event.body, EventBody::CacheExpired { .. }))
}

/// The columns of `sessions` in the database in `dir`.
fn columns(dir: &Path) -> Vec<String> {
    let db = Connection::open(dir.join("hub.db")).unwrap();
    let mut statement = db.prepare("SELECT name FROM pragma_table_info('sessions')").unwrap();
    statement.query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
}

/// A request while the agent is idle is told of at once, as of when it was sent, with whether the context is big
/// enough to tell of, which follows the context's reports. A hub that starts again has it as of then, and compares
/// the next request with it, so a prompt whose request wrote the conversation again found the cache expired.
#[tokio::test]
async fn what_the_cache_holds_is_told_as_of_its_request_and_kept_across_a_restart_for_the_next_to_compare_with() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = chat(&hub);
    says(&live, Report::Context { context: context(60_000) }).await;
    assert_eq!(listed(&hub).await, (None, None), "how full the context is says nothing of what the cache holds");

    says(&live, Report::Cache { request: request(0, 80_000) }).await;

    let (observed, filled) = listed(&hub).await;
    let held = filled.expect("the record filled");
    let told = ConversationCache { held: Some(held.clone()), worth_telling: true };
    assert_eq!(observed, Some(Observed::AsOf { value: told, as_of_ms: held.used_at_ms }));
    says(&live, Report::Context { context: context(20_000) }).await;
    let Some(Observed::AsOf { value, .. }) = listed(&hub).await.0 else { panic!("not known") };
    assert_eq!(value, ConversationCache { held: Some(held), worth_telling: false }, "the context went down");
    hub.observations_written().await;
    drop((live, hub));

    let again = hub_in_again(dir.path());
    let live = chat(&again);
    says(&live, Report::Cache { request: request(0, 80_000) }).await;

    assert!(found_expired(&again), "compared with what the hub before kept");
}

/// While the agent works, it makes a request every few seconds, and what the cache holds is told of only once it
/// stops, as the turn ends, which the daemon's tests see.
#[tokio::test]
async fn a_request_while_the_agent_works_isnt_told_of() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = chat(&hub);
    live.state.lock().progress.status = SessionStatus::Running;

    says(&live, Report::Cache { request: request(0, 80_000) }).await;

    assert_eq!(listed(&hub).await, (None, None));
}

/// A database from before what the cache holds left it opens with the column gone and the session as it was but for
/// it, and what the cache holds not known, as nothing is carried out of the database. So the first request after can't
/// be told to have found the cache expired, and records nothing of it, and is told of.
#[tokio::test]
async fn a_session_from_before_opens_with_what_its_cache_holds_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let held = PromptCache { used_at_ms: 5, ttl: CacheTtl::OneHour, model: None, tokens: 252_800 };
    let held = serde_json::to_string(&held).unwrap();
    let rows = format!(
        "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms, updated_at_ms,
                 title, prompt_cache, agent_session_id)
             VALUES ('s', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0, 'Kept', '{held}', 's');"
    );
    drop(store_from_before(dir.path(), BEFORE_SESSION_CACHE_OBSERVED, &rows));

    let hub = hub_in_again(dir.path());

    assert!(!columns(dir.path()).iter().any(|column| column == "prompt_cache"), "the column kept");
    assert_eq!(listed(&hub).await, (None, None), "not known");
    let live = chat(&hub);
    says(&live, Report::Cache { request: request(0, 80_000) }).await;
    assert!(!found_expired(&hub), "nothing to compare with");
    assert!(listed(&hub).await.0.is_some(), "known from the request");
}
