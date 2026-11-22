//! Each read of a session's events of some kinds reads only those events, already in order, from the index of kinds.
//! SQLite chooses how it reads, so this is what it chose: one that read the session's every event to find a few of
//! them still gives the right answer, only from far more of the database.

use super::super::recording::newest_own_failure;
use super::super::{Connection, lock};
use crate::store::tests::store_with_session;
use crate::store::{approvals, builds, lines};
use rusqlite::params_from_iter;
use rusqlite::types::Null;

/// What SQLite plans for `sql`, a step to a line.
#[expect(
    clippy::disallowed_methods,
    reason = "SQLite fixes the reply's columns, and a step's text is its fourth, `detail`"
)]
fn plan(conn: &Connection, sql: &str) -> Vec<String> {
    let mut statement = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let nulls = params_from_iter(std::iter::repeat_n(Null, statement.parameter_count()));
    statement.query_map(nulls, |row| row.get::<_, String>(3)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn a_read_of_some_kinds_of_event_reads_only_those_already_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let conn = lock(&store.reader);
    let reads = [
        ("prompts", lines::PROMPTS.to_string()),
        ("settlements", lines::SETTLEMENTS.to_string()),
        ("compactions", lines::COMPACTIONS.to_string()),
        ("newest start", builds::NEWEST_START.to_string()),
        ("request", approvals::REQUEST.to_string()),
        ("newest failure", newest_own_failure()),
    ];

    for (name, sql) in reads {
        let plan = plan(&conn, &sql);
        let of_events: Vec<_> =
            plan.iter().filter(|step| step.starts_with("SEARCH events") || step.starts_with("SCAN events")).collect();
        let by_kind = |step: &&String| step.contains("INDEX events_by_kind (session_id=? AND kind=?)");
        assert!(!of_events.is_empty() && of_events.iter().all(by_kind), "{name}: {plan:?}");
        assert!(plan.iter().all(|step| !step.contains("TEMP B-TREE")), "{name} sorts what it read: {plan:?}");
    }
}
