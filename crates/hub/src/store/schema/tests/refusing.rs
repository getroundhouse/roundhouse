//! Databases the hub refuses to migrate, which it leaves as they are.

use super::super::*;
use super::older::{made_from, older_database};
use crate::store::backup::files_in;

#[test]
fn a_database_from_a_newer_build_is_refused_and_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let newer = older_database(&path, &backups, KNOWN);
    newer.pragma_update(None, "user_version", i64::try_from(KNOWN + 1).unwrap()).unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    assert!(matches!(result, Err(StoreError::TooNew { .. })), "got {result:?}");
    assert_eq!(schema_version(&newer).unwrap(), KNOWN + 1);
    assert_eq!(files_in(&backups), Vec::<String>::new());
}

/// A database a migration short of the floor's, which 0.1.870's migrations made before they were one schema.
const SHORT_OF_THE_FLOOR: &str = include_str!("short_of_0.1.870.sql");

/// So the release at the floor can still migrate it, which is what the error says to install.
#[test]
fn a_database_older_than_the_floor_is_refused_and_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    made_from(&path, SHORT_OF_THE_FLOOR);
    let before = std::fs::read(&path).unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    let Err(refused @ StoreError::TooOld { found: 71, release: "0.1.870", migrations: 72 }) = result else {
        panic!("got {result:?}");
    };
    assert!(refused.to_string().contains("Install 0.1.870 and run it once"), "{refused}");
    assert!(std::fs::read(&path).unwrap() == before, "the database's file changed");
    assert_eq!(files_in(&backups), Vec::<String>::new());
}
