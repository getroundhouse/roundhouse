use parking_lot::Mutex;
use protocol::model::Rev;

use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_LOGINS;
use crate::store::tests::open_in;

/// A login of `agent`'s kind for `id`, connected at 100.
fn connecting<'a>(id: &'a AccountId, agent: AgentKind, file: &'a LoginFile) -> Connecting<'a> {
    Connecting { account: id, agent, name: "Work", file, connected_at_ms: 100, ends_at_ms: None }
}

#[test]
fn the_logins_held_for_a_kind_are_only_those_held_and_of_that_kind_the_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (gone, other) = (LoginFile::new(), LoginFile::new());
    // So the newest first is neither the rows' order, nor its reverse, nor the IDs' either way.
    let held = [("a", 300), ("m", 500), ("z", 100)].map(|(id, connected_at_ms)| {
        let file = LoginFile::new();
        store
            .connect_login(&Connecting { connected_at_ms, ..connecting(&id.into(), AgentKind::Claude, &file) })
            .unwrap();
        (AccountId::from(id), file, connected_at_ms)
    });
    store.connect_login(&connecting(&"gone".into(), AgentKind::Claude, &gone)).unwrap();
    store.clear_login(&"gone".into()).unwrap();
    store.connect_login(&connecting(&"other".into(), AgentKind::Unknown, &other)).unwrap();

    let listed: Vec<_> = store
        .held_logins(AgentKind::Claude)
        .unwrap()
        .into_iter()
        .map(|held| (held.account, held.file, held.login.connected_at_ms))
        .collect();

    let [a, m, z] = held;
    assert_eq!(listed, [m, a, z]);
}

#[test]
fn a_row_naming_a_file_the_hub_doesnt_make_or_with_no_connect_time_holds_no_login_and_is_listed_for_tidying() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (elsewhere, undated) = (LoginFile::new(), LoginFile::new());
    store.connect_login(&connecting(&"elsewhere".into(), AgentKind::Claude, &elsewhere)).unwrap();
    store.connect_login(&connecting(&"undated".into(), AgentKind::Claude, &undated)).unwrap();
    let conn = lock(&store.writer);
    conn.execute("UPDATE accounts SET login_file = '../hub.db' WHERE id = 'elsewhere'", []).unwrap();
    conn.execute("UPDATE accounts SET login_connected_at_ms = NULL WHERE id = 'undated'", []).unwrap();
    drop(conn);

    let held: Vec<_> = ["elsewhere", "undated"].map(|id| store.account(&id.into()).unwrap().unwrap().held_login).into();
    let rows: Vec<_> = ["elsewhere", "undated"].map(|id| store.login_row(&id.into()).unwrap()).into();

    assert_eq!(held, [None, None]);
    assert!(rows.iter().all(|row| *row == LoginRow { id_from: IdFrom::Hub(AgentKind::Claude), file: None }));
    assert!(store.held_logins(AgentKind::Claude).unwrap().is_empty());
    assert_eq!(store.login_files().unwrap().len(), 2);
    assert!(store.login_files().unwrap().iter().all(|(_, file)| file.is_none()));
}

#[test]
fn an_account_from_before_logins_were_held_has_none_and_its_id_is_its_adapters() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO accounts (id, email) VALUES ('claude-0123456789abcdef', 'ada@example.com');";

    let store = store_from_before(dir.path(), BEFORE_LOGINS, rows);

    let account = store.account(&"claude-0123456789abcdef".into()).unwrap().unwrap();
    assert_eq!((account.name, account.login_agent, account.held_login), (None, None, None));
    assert!(account.rev > Rev::default());
    let row = store.login_row(&account.id).unwrap();
    assert_eq!(row, LoginRow { id_from: IdFrom::Adapter, file: None });
}

#[test]
fn every_connection_keeps_its_checkpoints_in_order_on_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let checkpoint_fullfsync = |conn: &Mutex<rusqlite::Connection>| -> i64 {
        lock(conn).pragma_query_value(None, "checkpoint_fullfsync", |row| row.get(0)).unwrap()
    };

    let set = [&store.writer, &store.reader, &store.finder].map(checkpoint_fullfsync);

    assert_eq!(set, [1, 1, 1], "whichever connection closes last checkpoints");
}

#[test]
fn a_commit_that_has_to_last_is_made_with_full_syncs_and_the_writer_syncs_as_usual_after_even_one_that_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let syncs = |conn: &rusqlite::Connection| -> (i64, i64) {
        let read = |pragma: &str| conn.pragma_query_value(None, pragma, |row| row.get(0)).unwrap();
        (read("synchronous"), read("fullfsync"))
    };

    let during = store.lasting(|tx| Ok(syncs(tx))).unwrap();
    let failed = store.lasting(|_| Err::<(), _>(rusqlite::Error::QueryReturnedNoRows));
    let after = syncs(&lock(&store.writer));

    assert_eq!(during, (2, 1), "FULL, with fullfsync");
    assert!(failed.is_err());
    assert_eq!(after, (1, 0), "NORMAL, without");
}
