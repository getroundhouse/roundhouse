//! What a hub observes of a chat, spelled out: how full its context is and its to-do list, its tool servers, what the
//! API's cache holds of it, and what each of its waits last read, as a hub sends each to a client of a version that reads
//! it so.

use protocol::capability::output::TodoList;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{
    CacheTtl, ContextUsage, ConversationCache, Observation, ObservationKey, Observed, PromptCache, Rev, WaitRead,
};

use crate::keeps_its_form;

/// How full a session's context is, and its to-do list, which a hub sends at the session's level to a client of a
/// version that reads them so: as a notice, as a removal, and in the session's list.
#[test]
fn a_session_s_context_and_todos_keep_their_form_as_notices_and_in_its_list() {
    let context = ContextUsage {
        used_tokens: 60_000,
        max_tokens: 200_000,
        model_tokens: None,
        max_set_by: None,
        parts: Vec::new(),
    };
    let context = Observed::AsOf { value: context, as_of_ms: 1_795_230_000_000 };
    let copy = Observation::SessionContext { session_id: "s".into(), context, rev: Rev::new(5) };
    let todos = TodoList { items: Vec::new(), note: Some("Nothing left".into()) };
    let todos = Observation::SessionTodos {
        session_id: "s".into(),
        todos: Observed::AsOf { value: todos, as_of_ms: 1_795_230_000_000 },
        rev: Rev::new(6),
    };
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::SessionContext { session_id: "s".into() },
        rev: Rev::new(7),
    };
    let listed = Listed::Session(SessionRecords {
        session_id: "s".into(),
        subagents: Vec::new(),
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        observations: vec![todos.clone()],
        rev: Rev::new(8),
    });

    keeps_its_form(
        &HubNotice::Observation(copy),
        concat!(
            r#"{"notice":"observation","kind":"session_context","session_id":"s","context":{"freshness":"as_of","#,
            r#""value":{"used_tokens":60000,"max_tokens":200000,"parts":[]},"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &HubNotice::Observation(todos),
        concat!(
            r#"{"notice":"observation","kind":"session_todos","session_id":"s","todos":{"freshness":"as_of","#,
            r#""value":{"items":[],"note":"Nothing left"},"as_of_ms":1795230000000},"rev":6}"#
        ),
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"session_context","session_id":"s"},"rev":7}"#,
    );
    keeps_its_form(
        &HubFrame::Listed { subscription_id: 2, listed },
        concat!(
            r#"{"frame":"listed","subscription_id":2,"listed":{"level":"session","session_id":"s","subagents":[],"#,
            r#""proposals":[],"replies_heard":[],"observations":[{"kind":"session_todos","session_id":"s","#,
            r#""todos":{"freshness":"as_of","value":{"items":[],"note":"Nothing left"},"as_of_ms":1795230000000},"#,
            r#""rev":6}],"rev":8}}"#
        ),
    );
}

/// A session's tool servers, which a hub sends at the session's level to a client of a version that reads them, as a
/// notice and as a removal.
#[test]
fn a_session_s_tool_servers_keep_their_form_as_a_notice_and_a_removal() {
    let docs = ToolServerState { name: "docs".into(), state: ServerState::Connected, error: None, tools: Some(2) };
    let gone = ToolServerState {
        name: "gone".into(),
        state: ServerState::Failed,
        error: Some("connection refused".into()),
        tools: None,
    };
    let copy = Observation::SessionToolServers {
        session_id: "s".into(),
        servers: Observed::AsOf { value: vec![docs, gone], as_of_ms: 1_795_230_000_000 },
        rev: Rev::new(5),
    };
    let removed = HubNotice::ObservationRemoved {
        observation: ObservationKey::SessionToolServers { session_id: "s".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &HubNotice::Observation(copy),
        concat!(
            r#"{"notice":"observation","kind":"session_tool_servers","session_id":"s","servers":{"freshness":"as_of","#,
            r#""value":[{"name":"docs","state":"connected","tools":2},{"name":"gone","state":"failed","#,
            r#""error":"connection refused"}],"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &removed,
        r#"{"notice":"observation_removed","observation":{"kind":"session_tool_servers","session_id":"s"},"rev":6}"#,
    );
}

/// What the API's cache holds of a session's conversation, which a hub sends at its own level to a client of a version
/// that reads it so, as a notice, and as a cache that holds none of it.
#[test]
fn what_a_session_s_cache_holds_keeps_its_form_as_a_notice() {
    let held = PromptCache { used_at_ms: 1_795_230_000_000, ttl: CacheTtl::OneHour, model: None, tokens: 252_800 };
    let cache =
        |held| Observed::AsOf { value: ConversationCache { held, worth_telling: true }, as_of_ms: 1_795_230_000_000 };
    let copy = |held| {
        HubNotice::Observation(Observation::SessionCache {
            session_id: "s".into(),
            cache: cache(held),
            rev: Rev::new(5),
        })
    };

    keeps_its_form(
        &copy(Some(held)),
        concat!(
            r#"{"notice":"observation","kind":"session_cache","session_id":"s","cache":{"freshness":"as_of","#,
            r#""value":{"held":{"used_at_ms":1795230000000,"ttl":"one_hour","model":null,"tokens":252800},"#,
            r#""worth_telling":true},"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &copy(None),
        concat!(
            r#"{"notice":"observation","kind":"session_cache","session_id":"s","cache":{"freshness":"as_of","#,
            r#""value":{"worth_telling":true},"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
}

/// What a wait last read, which a hub sends at the wait's session's level to a client of a version that reads it so, as
/// a notice and as its removal.
#[test]
fn what_a_wait_last_read_keeps_its_form_as_a_notice_and_a_removal() {
    let read = WaitRead {
        pull_requests: Vec::new(),
        observed_at_ms: 1_795_230_000_000,
        read_error: Some("offline".into()),
        last_attempt_at_ms: 1_795_230_060_000,
        affected: Vec::new(),
    };
    let read = Observed::AsOf { value: read, as_of_ms: 1_795_230_000_000 };
    let copy = HubNotice::Observation(Observation::WaitRead {
        session_id: "s".into(),
        wait: "a".into(),
        read,
        rev: Rev::new(5),
    });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WaitRead { session_id: "s".into(), wait: "a".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        concat!(
            r#"{"notice":"observation","kind":"wait_read","session_id":"s","wait":"a","read":{"freshness":"as_of","#,
            r#""value":{"pull_requests":[],"observed_at_ms":1795230000000,"read_error":"offline","#,
            r#""last_attempt_at_ms":1795230060000},"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"wait_read","session_id":"s","wait":"a"},"rev":6}"#,
    );
}
