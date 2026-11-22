//! The floor's database: one that 0.1.870 left, which this build migrates, and a new one, which starts from its schema.

use protocol::model::Seq;
use rusqlite::types::Value;

use super::super::*;
use super::older::made_from;
use crate::store::backup::files_in;
use crate::store::{Store, WorkspacePlacement};

/// A database as 0.1.870's migrations left it, with rows in it, which those migrations made before they were one
/// schema.
const LEFT_BY_THE_FLOOR: &str = include_str!("left_by_0.1.870.sql");

/// What `sql` reads, with `name` as its parameter if it has one, each row as its values say it.
#[expect(
    clippy::disallowed_methods,
    reason = "it reads every column of SQLite's own tables of a schema and its pragmas, whose columns SQLite fixes"
)]
fn read(conn: &Connection, sql: &str, name: Option<&str>) -> Vec<String> {
    let mut statement = conn.prepare(sql).unwrap();
    let columns = statement.column_count();
    let rows = statement.query_map(rusqlite::params_from_iter(name), |row| {
        (0..columns).map(|column| row.get::<_, Value>(column)).collect::<Result<Vec<_>, _>>()
    });
    rows.unwrap().map(|row| format!("{:?}", row.unwrap())).collect()
}

/// Everything SQLite keeps of the database's schema, but where its pages are: how many migrations have run, each
/// table, index and trigger as SQLite keeps it, and each table's columns, foreign keys and indexes, and each index's
/// columns.
fn schema_of(path: &std::path::Path) -> Vec<String> {
    let conn = open_connection(path).unwrap();
    let mut schema = vec![format!("user_version {}", schema_version(&conn).unwrap())];
    schema.extend(read(&conn, "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name", None));
    let named = |kind| read(&conn, "SELECT name FROM sqlite_master WHERE type = ?1 ORDER BY name", Some(kind));
    for table in named("table") {
        let table = table.trim_start_matches("[Text(\"").trim_end_matches("\")]").to_string();
        for pragma in ["pragma_table_xinfo", "pragma_foreign_key_list", "pragma_index_list"] {
            schema.push(format!("{pragma} {table}"));
            schema.extend(read(&conn, &format!("SELECT * FROM {pragma}(?1) ORDER BY 1, 2"), Some(&table)));
        }
    }
    for index in named("index") {
        let index = index.trim_start_matches("[Text(\"").trim_end_matches("\")]").to_string();
        schema.push(format!("pragma_index_xinfo {index}"));
        schema.extend(read(&conn, "SELECT * FROM pragma_index_xinfo(?1)", Some(&index)));
    }
    schema
}

/// A new database starts from the floor's schema, so it's what 0.1.870 left, as SQLite keeps it.
#[test]
fn a_new_database_starts_as_the_floors_release_left_one() {
    let dir = tempfile::tempdir().unwrap();
    let (new, left) = (dir.path().join("new.db"), dir.path().join("left.db"));
    let backups = dir.path().join("backups");

    migrate_through(&mut open_connection(&new).unwrap(), &backups, &[]).unwrap();
    made_from(&left, LEFT_BY_THE_FLOOR);

    assert_eq!(schema_version(&open_connection(&new).unwrap()).unwrap(), STARTS_AT.migrations);
    assert_eq!(schema_of(&new), schema_of(&left));
}

/// What 0.1.870 left opens, is copied before it migrates, and keeps what it holds.
#[test]
fn a_database_the_floors_release_left_migrates_with_what_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    made_from(&path, LEFT_BY_THE_FLOOR);

    let store = Store::open(&path, &backups).unwrap();

    assert_eq!(schema_version(&open_connection(&path).unwrap()).unwrap(), KNOWN);
    assert_eq!(files_in(&backups).len(), 1, "copied before it migrated");
    let workspace = store.get_workspace(&"w".into()).unwrap();
    assert_eq!(workspace.title.map(|title| title.to_string()).as_deref(), Some("Tidy the parser"));
    let link = workspace.pull_request_link.and_then(|linked| linked.link);
    assert_eq!(link.map(|link| link.url).as_deref(), Some("https://github.com/ada/rails/pull/7"));
    assert_eq!(store.workspace_placement(&"w".into()).unwrap(), WorkspacePlacement::local_checkout("/src/rails"));
    let session = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(session.title.map(|title| title.to_string()).as_deref(), Some("Tidy the parser"));
    assert_eq!(store.read_events(&"s".into(), Seq::new(1), 10, u64::MAX, false).unwrap().len(), 2);
    let records = store.list_records().unwrap();
    assert_eq!(records.accounts.len(), 1);
    assert!(records.github_accounts.is_empty(), "an account with no login goes, as a migration since the floor has it");
}
