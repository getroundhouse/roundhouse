use protocol::model::{IssueNumber, Level, Observation, Observed, PullRequest, Reference, ReferenceRead};
use rusqlite::Connection;

use super::super::file::prepare_forms;
use super::super::keepers::Keeper;
use super::super::kinds::{Form, Kind, WorkspaceChanges, WorkspaceLines};
use super::seen::{as_of, changes, lines, listed, of_w};
use crate::github_cache::Item;
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::columns::OneValue;
use crate::store::tests::another_workspace;

/// A hub that starts again lists each observation as of its last read, under a revision newer than any the hub before
/// gave, so a client of that hub takes it, and nothing loaded is current, since nothing keeps it up to date yet.
#[tokio::test]
async fn an_observation_is_kept_across_a_restart_as_of_its_last_read_under_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_kept_by::<WorkspaceLines>("w".into(), Keeper::Hub).await.result().unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.result().unwrap();
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 2_000).await.result().unwrap();
    let (_, before) = listed(&hub).await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let (observations, rev) = listed(&again).await;
    let [observation] = observations.try_into().unwrap();
    assert!(observation.rev() > before, "{observation:?} after a list at {before}");
    assert_eq!(observation, of_w(as_of(lines(3, 1), 2_000), observation.rev()));
    assert!(rev >= observation.rev());
}

/// A read that finds what the file keeps writes only when it was read, and not what it says again, which for a
/// workspace's changed files can be large. What the file keeps is changed behind the store's back here, so a hub that
/// starts again shows which was written.
#[tokio::test]
async fn a_read_that_finds_the_same_writes_only_when_it_was_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceChanges>("w".into(), changes(3, 1), 1_000).await.result().unwrap();
    hub.observations_written().await;
    let file = Connection::open(dir.path().join("github-cache.db")).unwrap();
    let elsewhere = serde_json::to_string(&changes(9, 9)).unwrap();
    file.execute("UPDATE observed_workspace_changes SET said = ?", [elsewhere]).unwrap();

    hub.observation_read::<WorkspaceChanges>("w".into(), changes(3, 1), 2_000).await.result().unwrap();
    drop(hub);

    let again = hub_in_again(dir.path());
    let (observations, _) = again.observations_at(&Level::Workspace { id: "w".into() }).await.unwrap();
    let [kept] = observations.try_into().unwrap();
    let read_then = Observed::AsOf { value: changes(9, 9), as_of_ms: 2_000 };
    assert_eq!(kept, Observation::WorkspaceChanges { workspace_id: "w".into(), changes: read_then, rev: kept.rev() });
}

/// A hub that starts on a file whose version for one kind isn't its own, as a build that changed that kind's form
/// left it, empties that kind alone, and keeps the others, GitHub's included, each under a version of its own. One
/// whose table of versions is of another form empties the observations, and keeps GitHub's.
#[tokio::test]
async fn a_kind_of_another_form_is_emptied_and_the_others_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.result().unwrap();
    let item = read_a_pull_request(&hub);
    drop(hub);
    let file = Connection::open(dir.path().join("github-cache.db")).unwrap();
    file.execute("UPDATE observation_forms SET version = version + 1 WHERE observed = 'workspace_lines'", []).unwrap();

    let again = hub_in_again(dir.path());

    assert_eq!(listed(&again).await.0, [], "lines of another form");
    assert!(matches!(again.github_cache.summary(&item), Some(ReferenceRead::Found { .. })), "GitHub's kept");
    again.observation_read::<WorkspaceLines>("w".into(), lines(4, 0), 2_000).await.result().unwrap();
    drop(again);
    file.pragma_update(None, "user_version", 3).unwrap();

    let once_more = hub_in_again(dir.path());

    assert_eq!(once_more.github_cache.summary(&item), None, "GitHub's of another form");
    let [observation] = listed(&once_more).await.0.try_into().unwrap();
    assert_eq!(observation, of_w(as_of(lines(4, 0), 2_000), observation.rev()), "lines kept");
    read_a_pull_request(&once_more);
    drop(once_more);
    file.execute_batch("DROP TABLE observation_forms; CREATE TABLE observation_forms (observed TEXT PRIMARY KEY)")
        .unwrap();

    let versions_of_another_form = hub_in_again(dir.path());

    assert_eq!(listed(&versions_of_another_form).await.0, [], "the observations emptied");
    let summary = versions_of_another_form.github_cache.summary(&item);
    assert!(matches!(summary, Some(ReferenceRead::Found { .. })), "GitHub's kept");
    let made: i64 = file
        .query_row("SELECT version FROM observation_forms WHERE observed = 'workspace_lines'", [], OneValue::read)
        .unwrap();
    assert_eq!(made, WorkspaceLines::FORM.version, "made again");
}

/// Keeps a summary of a pull request in `hub`'s cache of GitHub, as a read of it does, and returns which it is.
fn read_a_pull_request(hub: &Hub) -> Item {
    let read = protocol::fixtures::pull_request(7);
    let summary = PullRequest { history: None, stack: None, checks: None, ..read };
    let item = Item::new(&summary.repository, IssueNumber::try_from(summary.number.get()).unwrap());
    hub.github_cache.keep(vec![(item.clone(), Ok(Some(Reference::PullRequest(summary))))], crate::store::now_ms());
    item
}

/// Each kind's table is emptied only when its own version isn't this build's, or it can't be made, as where something
/// else has its name, and one of a kind this build doesn't know, as a newer build left it, is left as it is, for
/// whichever kinds there are.
#[test]
fn each_kind_s_table_is_emptied_only_when_its_own_version_changes_or_it_can_t_be_made() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = Connection::open(dir.path().join("cache.db")).unwrap();
    let form = |table, version| Form { table, version };
    prepare_forms(&mut conn, &[form("a", 1), form("b", 1), form("newer", 1)]).unwrap();
    for table in ["a", "b", "newer"] {
        keep_one(&conn, table);
    }

    prepare_forms(&mut conn, &[form("a", 2), form("b", 1)]).unwrap();

    assert_eq!(rows(&conn, "a"), 0, "of another version");
    assert_eq!(rows(&conn, "b"), 1);
    assert_eq!(rows(&conn, "newer"), 1, "not this build's");
    keep_one(&conn, "a");
    prepare_forms(&mut conn, &[form("a", 2), form("b", 1)]).unwrap();
    assert_eq!((rows(&conn, "a"), rows(&conn, "b")), (1, 1), "each of this build's version");
    conn.execute_batch(
        "DROP TABLE observed_b; CREATE VIEW observed_b AS SELECT 1; DELETE FROM observation_forms WHERE observed = 'b'",
    )
    .unwrap();

    prepare_forms(&mut conn, &[form("a", 2), form("b", 1)]).unwrap();

    assert_eq!((rows(&conn, "a"), rows(&conn, "b")), (1, 0), "b made again in place of what had its name");
}

/// Keeps an observation in `table`.
fn keep_one(conn: &Connection, table: &str) {
    conn.execute(&format!(r#"INSERT INTO observed_{table} (of, said, read_at_ms) VALUES ('"x"', '1', 1)"#), [])
        .unwrap();
}

/// How many observations `table` keeps.
fn rows(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM observed_{table}"), [], OneValue::read).unwrap()
}

/// A hub that starts again forgets what the file keeps of a workspace its database doesn't have, from the file too, as
/// a hub that stopped between removing the workspace and forgetting its observations leaves it, or a file that refused
/// the removal, so no client is listed it.
#[tokio::test]
async fn a_hub_that_starts_again_forgets_the_observations_of_a_workspace_that_s_gone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    another_workspace(&hub.store, "gone");
    hub.observation_read::<WorkspaceLines>("w".into(), lines(3, 1), 1_000).await.result().unwrap();
    hub.observation_read::<WorkspaceLines>("gone".into(), lines(4, 0), 1_000).await.result().unwrap();
    hub.observation_read::<WorkspaceChanges>("gone".into(), changes(4, 0), 1_000).await.result().unwrap();
    hub.observations_written().await;
    drop(hub);
    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    db.execute("DELETE FROM workspaces WHERE id = 'gone'", []).unwrap();

    let again = hub_in_again(dir.path());

    let [kept] = listed(&again).await.0.try_into().unwrap();
    assert_eq!(kept, of_w(as_of(lines(3, 1), 1_000), kept.rev()));
    let (gone, _) = again.observations_at(&Level::Workspace { id: "gone".into() }).await.unwrap();
    assert_eq!(gone, []);
    drop(again);
    let file = Connection::open(dir.path().join("github-cache.db")).unwrap();
    assert_eq!((rows(&file, "workspace_lines"), rows(&file, "workspace_changes")), (1, 0), "forgotten from the file");
}
