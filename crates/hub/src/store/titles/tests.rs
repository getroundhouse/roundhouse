use protocol::model::Workspace;

use super::*;
use crate::store::schema::{self, BEFORE_TITLES_TIDIED};
use crate::store::tests::{idle_session, store_with_session, workspace_w};
use crate::store::{ByColumn, Columns, WorkspacePlacement, columns};

fn title(text: &str) -> Title {
    Title::typed(text).unwrap()
}

#[test]
fn a_first_line_only_fills_a_missing_title() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let workspace = store.retitle_workspace(&"w".into(), &title("Fix the login"), Retitle::FirstLine).unwrap();
    let later = store.retitle_workspace(&"w".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();
    let chat = store.retitle_session(&"s".into(), &title("Fix the login"), Retitle::FirstLine).unwrap();
    let later_chat = store.retitle_session(&"s".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();

    assert!(workspace.changed && chat.changed);
    assert!(!later.changed && !later_chat.changed);
    let kept = (later.record.title.as_deref(), later.record.title_source);
    assert_eq!(kept, (Some("Fix the login"), TitleSource::FirstLine));
    assert_eq!(later_chat.record.title.as_deref(), Some("Fix the login"));
    assert_eq!(later.record.rev, workspace.record.rev, "a title that doesn't apply changes nothing to announce");
}

#[test]
fn a_title_typed_by_hand_replaces_a_first_line_and_stays() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.retitle_workspace(&"w".into(), &title("Fix the login"), Retitle::FirstLine).unwrap();
    store.retitle_session(&"s".into(), &title("Fix the login"), Retitle::FirstLine).unwrap();

    let typed = store.retitle_workspace(&"w".into(), &title("Login bug"), Retitle::ByHand).unwrap();
    let typed_chat = store.retitle_session(&"s".into(), &title("First try"), Retitle::ByHand).unwrap();
    store.retitle_workspace(&"w".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();
    store.retitle_session(&"s".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();

    assert!(typed.changed && typed_chat.changed);
    let workspace = store.get_workspace(&"w".into()).unwrap();
    assert_eq!((workspace.title.as_deref(), workspace.title_source), (Some("Login bug"), TitleSource::ByHand));
    let chat = store.get_session(&"s".into()).unwrap().session;
    assert_eq!((chat.title.as_deref(), chat.title_source), (Some("First try"), TitleSource::ByHand));
}

#[test]
fn a_generated_title_replaces_only_a_first_line_or_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    store.retitle_workspace(&"w".into(), &title("fix the login"), Retitle::FirstLine).unwrap();
    let generated = store.retitle_workspace(&"w".into(), &title("Login redirect"), Retitle::Generated).unwrap();
    let untitled = store.retitle_session(&"s".into(), &title("Login redirect"), Retitle::Generated).unwrap();
    store.retitle_session(&"s".into(), &title("Login bug"), Retitle::ByHand).unwrap();
    let typed = store.retitle_session(&"s".into(), &title("Login redirect"), Retitle::Generated).unwrap();
    let again = store.retitle_workspace(&"w".into(), &title("Redirect loop"), Retitle::Generated).unwrap();

    assert!(generated.changed);
    let workspace = generated.record;
    assert_eq!((workspace.title.as_deref(), workspace.title_source), (Some("Login redirect"), TitleSource::Generated));
    assert!(untitled.changed, "a prompt of only images leaves no first line to replace");
    assert_eq!(untitled.record.title_source, TitleSource::Generated);
    assert!(!typed.changed, "a title typed by hand stays");
    assert_eq!((typed.record.title.as_deref(), typed.record.title_source), (Some("Login bug"), TitleSource::ByHand));
    assert!(!again.changed, "the first generated title stays too");
}

fn regenerated(over: Option<&Title>, set: TitleSource) -> Retitle<'_> {
    Retitle::Regenerated { over, set }
}

#[test]
fn a_regenerated_title_replaces_the_one_it_was_asked_over_however_that_was_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (typed, first) = (title("Login bug"), title("Fix login redirect"));
    store.retitle_workspace(&"w".into(), &typed, Retitle::ByHand).unwrap();

    let over_typed = regenerated(Some(&typed), TitleSource::ByHand);
    let new = store.retitle_workspace(&"w".into(), &first, over_typed).unwrap();
    let over_generated = regenerated(Some(&first), TitleSource::Generated);
    let again = store.retitle_workspace(&"w".into(), &title("Fix redirect loop"), over_generated).unwrap();

    assert!(new.changed, "over a title typed by hand, since someone asked");
    assert_eq!(
        (new.record.title.as_deref(), new.record.title_source),
        (Some("Fix login redirect"), TitleSource::Generated)
    );
    assert!(again.changed, "and over a generated one");
    assert_eq!(again.record.title.as_deref(), Some("Fix redirect loop"));
}

#[test]
fn a_regenerated_title_leaves_one_set_since_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (first_line, new) = (title("fix the login"), title("Fix login redirect"));
    store.retitle_workspace(&"w".into(), &first_line, Retitle::FirstLine).unwrap();
    let typed = store.retitle_workspace(&"w".into(), &title("Login bug"), Retitle::ByHand).unwrap();

    let over_first_line = regenerated(Some(&first_line), TitleSource::FirstLine);
    let late = store.retitle_workspace(&"w".into(), &new, over_first_line).unwrap();
    let untitled = store.retitle_workspace(&"w".into(), &new, regenerated(None, TitleSource::FirstLine)).unwrap();
    let chat = store.retitle_session(&"s".into(), &new, regenerated(None, TitleSource::FirstLine)).unwrap();

    assert!(!late.changed && !untitled.changed);
    assert_eq!((late.record.title.as_deref(), late.record.title_source), (Some("Login bug"), TitleSource::ByHand));
    assert_eq!(late.record.rev, typed.record.rev, "a title that doesn't apply changes nothing to announce");
    assert!(chat.changed, "asked while there was no title, and there's still none");
}

/// Renaming a workspace to what it's already called is someone keeping a generated title, so it stays too.
#[test]
fn a_regenerated_title_leaves_the_same_words_typed_by_hand_since() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let generated = title("Fix login redirect");
    store.retitle_workspace(&"w".into(), &generated, Retitle::FirstLine).unwrap();
    store.retitle_workspace(&"w".into(), &generated, Retitle::ByHand).unwrap();

    let over_first_line = regenerated(Some(&generated), TitleSource::FirstLine);
    let late = store.retitle_workspace(&"w".into(), &title("Something else"), over_first_line).unwrap();

    assert!(!late.changed);
    let kept = (late.record.title.as_deref(), late.record.title_source);
    assert_eq!(kept, (Some("Fix login redirect"), TitleSource::ByHand));
}

columns! {
    /// What [`titles_stored_before_they_were_tidied_are_tidied_once`] compares of a row: its title, byte for byte,
    /// and its revision.
    struct StoredTitle { kind = "typeof(title)", bytes = "quote(CAST(title AS BLOB))", rev }
}

/// A title an older build stored as it found it is stored as it reads, and one with nothing left once tidied is none.
/// One already tidy, and one that isn't text, which no build wrote, are left as they were, revision and all. A title
/// asked for over a tidied one replaces it.
#[test]
fn titles_stored_before_they_were_tidied_are_tidied_once() {
    let dir = tempfile::tempdir().unwrap();
    let (path, backups) = (dir.path().join("hub.db"), dir.path().join("backups"));
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let workspace =
        "INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms, title)";
    let session = "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms,
        updated_at_ms, title)";
    let rows = format!(
        "INSERT INTO repos (id, name, path, created_at_ms) VALUES ('r', 'repo', '/repo', 0);
         {workspace} VALUES ('w', 'r', 'alder', 'alder', 'ready', 'local', 'w', 0, ' Login\tbug\u{202E} '),
             ('tidy', 'r', 'birch', 'birch', 'ready', 'local', 'tidy', 0, 'Fix the login'),
             ('blank', 'r', 'cedar', 'cedar', 'ready', 'local', 'blank', 0, ' \u{200B}\n'),
             ('blob', 'r', 'elm', 'elm', 'ready', 'local', 'blob', 0, X'4c6f67696e'),
             ('bytes', 'r', 'fir', 'fir', 'ready', 'local', 'bytes', 0, CAST(X'4c6f67696eff' AS TEXT));
         UPDATE workspaces SET title_source = 'by_hand' WHERE id = 'w';
         {session} VALUES ('s', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0, 'Fix the\n  login'),
             ('t', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0, 'Fix the login');"
    );
    let left_alone = [("workspaces", "tidy"), ("workspaces", "blob"), ("workspaces", "bytes"), ("sessions", "t")];
    let stored = |conn: &rusqlite::Connection, (table, id): (&str, &str)| -> (String, String, i64) {
        let c = StoredTitle::at(0);
        let read = format!("SELECT {} FROM {table} WHERE id = ?", StoredTitle::LIST);
        conn.query_row(&read, [id], |row| Ok((row.at(c.kind)?, row.at(c.bytes)?, row.at(c.rev)?))).unwrap()
    };
    let as_left = {
        let mut conn = schema::open_connection(&path).unwrap();
        schema::migrate_to(&mut conn, &backups, BEFORE_TITLES_TIDIED).unwrap();
        conn.execute_batch(&rows).unwrap();
        left_alone.map(|row| stored(&conn, row))
    };

    let store = Store::open(&path, &backups).unwrap();

    let workspace = store.get_workspace(&"w".into()).unwrap();
    assert_eq!((workspace.title.as_deref(), workspace.title_source), (Some("Login bug"), TitleSource::ByHand));
    assert_eq!(store.get_workspace(&"blank".into()).unwrap().title, None);
    assert_eq!(store.get_session(&"s".into()).unwrap().session.title.as_deref(), Some("Fix the login"));
    let conn = lock(&store.writer);
    assert_eq!(stored(&conn, ("workspaces", "blank")).1, "NULL");
    assert_eq!(left_alone.map(|row| stored(&conn, row)), as_left);
    drop(conn);
    let over = regenerated(workspace.title.as_ref(), TitleSource::ByHand);
    assert!(store.retitle_workspace(&"w".into(), &title("Fix login redirect"), over).unwrap().changed);
}

/// A title reads as it's stored, with no tidying, so one a database changed by hand holds untidy reads as it is, for a
/// client to tidy as it reads it off the wire.
#[test]
fn a_title_reads_as_its_stored() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    lock(&store.writer).execute("UPDATE workspaces SET title = ' Login  bug' WHERE id = 'w'", []).unwrap();

    let read = store.get_workspace(&"w".into()).unwrap();

    assert_eq!(read.title.as_deref(), Some(" Login  bug"));
}

#[test]
fn retitling_something_missing_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let asked = regenerated(None, TitleSource::FirstLine);
    for how in [Retitle::FirstLine, Retitle::Generated, asked, Retitle::ByHand] {
        let workspace = store.retitle_workspace(&"missing".into(), &title("x"), how);
        let chat = store.retitle_session(&"missing".into(), &title("x"), how);

        assert!(matches!(workspace, Err(StoreError::NotFound { kind: "workspace", .. })), "{how:?}");
        assert!(matches!(chat, Err(StoreError::NotFound { kind: "session", .. })), "{how:?}");
    }
}

/// Each rule decides whether one record takes a title, so every other one keeps its own, whether it has none or still
/// shows a first line.
#[test]
fn retitling_one_record_leaves_the_others_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for (id, name) in [("w2", "birch"), ("w3", "cedar")] {
        let other = Workspace {
            id: id.into(),
            name: name.into(),
            branch: name.into(),
            instance_id: id.into(),
            ..workspace_w(1)
        };
        store.insert_workspace(&other, &WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    }
    for id in ["t", "u"] {
        let mut chat = idle_session(1);
        (chat.session.id, chat.agent_session_id) = (id.into(), Some(id.into()));
        store.insert_session(&chat, None).unwrap();
    }
    store.retitle_workspace(&"w3".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();
    store.retitle_session(&"u".into(), &title("Write the docs"), Retitle::FirstLine).unwrap();
    let others = |store: &Store| {
        let workspaces = ["w2", "w3"].map(|id| {
            let workspace = store.get_workspace(&id.into()).unwrap();
            (workspace.title, workspace.title_source, workspace.rev)
        });
        let chats = ["t", "u"].map(|id| {
            let chat = store.get_session(&id.into()).unwrap().session;
            (chat.title, chat.title_source, chat.rev)
        });
        (workspaces, chats)
    };
    let before = others(&store);

    for how in [Retitle::FirstLine, Retitle::Generated, regenerated(None, TitleSource::FirstLine), Retitle::ByHand] {
        store.retitle_workspace(&"w".into(), &title("Fix the login"), how).unwrap();
        store.retitle_session(&"s".into(), &title("Fix the login"), how).unwrap();
    }

    assert_eq!(others(&store), before);
}

/// Where a chat's title came from is part of its record, so a change to that alone gets announced too. Every
/// retitle sets both columns, so only a write of its own shows the trigger names it.
#[test]
fn where_a_chat_title_came_from_moves_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;

    lock(&store.writer).execute("UPDATE sessions SET title_source = 'by_hand' WHERE id = 's'", []).unwrap();

    assert!(store.get_session(&"s".into()).unwrap().session.rev > before);
}
