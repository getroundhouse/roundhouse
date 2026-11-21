//! Databases the hub refuses to migrate, which it leaves as they are.

use super::super::*;
use super::older::older_database;
use crate::store::backup::files_in;
use update_feed::Floor;

#[test]
fn a_database_from_a_newer_build_is_refused_and_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let newer = older_database(&path, &backups, MIGRATIONS.len());
    newer.pragma_update(None, "user_version", i64::try_from(MIGRATIONS.len() + 1).unwrap()).unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    assert!(matches!(result, Err(StoreError::TooNew { .. })), "got {result:?}");
    assert_eq!(schema_version(&newer).unwrap(), MIGRATIONS.len() + 1);
    assert_eq!(files_in(&backups), Vec::<String>::new());
}

/// A floor at the database the third migration left, as a build might have once the first three are one.
const AT_THREE: Floor = Floor { release: "0.1.300", migrations: 3 };

/// So the release at the floor can still migrate it, which is what the error says to install.
#[test]
fn a_database_older_than_the_floor_is_refused_and_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let older = older_database(&path, &backups, 2);

    let result = migrate_through(&mut open_connection(&path).unwrap(), &backups, MIGRATIONS, Some(AT_THREE));

    let Err(refused @ StoreError::TooOld { found: 2, release: "0.1.300", migrations: 3 }) = result else {
        panic!("got {result:?}");
    };
    assert!(refused.to_string().contains("Install 0.1.300 and run it once"), "{refused}");
    assert_eq!(schema_version(&older).unwrap(), 2);
    assert_eq!(files_in(&backups), Vec::<String>::new());
}

/// A new database holds nothing to migrate, so it isn't older than any floor.
#[test]
fn a_new_database_and_one_at_the_floor_migrate() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let new = dir.path().join("new.db");
    let at_floor = dir.path().join("at-floor.db");
    let _at_floor = older_database(&at_floor, &backups, 3);

    for path in [&new, &at_floor] {
        migrate_through(&mut open_connection(path).unwrap(), &backups, MIGRATIONS, Some(AT_THREE)).unwrap();

        assert_eq!(schema_version(&open_connection(path).unwrap()).unwrap(), MIGRATIONS.len(), "{}", path.display());
    }
}
