use super::*;
use crate::store::tests::store_with_session;
use crate::store::{older, schema};

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
fn a_generated_title_replaces_only_a_first_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let untitled = store.retitle_workspace(&"w".into(), &title("Login redirect"), Retitle::Generated).unwrap();
    store.retitle_workspace(&"w".into(), &title("fix the login"), Retitle::FirstLine).unwrap();
    let generated = store.retitle_workspace(&"w".into(), &title("Login redirect"), Retitle::Generated).unwrap();
    store.retitle_session(&"s".into(), &title("Login bug"), Retitle::ByHand).unwrap();
    let typed = store.retitle_session(&"s".into(), &title("Login redirect"), Retitle::Generated).unwrap();

    assert!(!untitled.changed, "nothing to replace before the first line");
    assert!(generated.changed);
    let workspace = generated.record;
    assert_eq!((workspace.title.as_deref(), workspace.title_source), (Some("Login redirect"), TitleSource::Generated));
    assert!(!typed.changed, "a title typed by hand stays");
    assert_eq!((typed.record.title.as_deref(), typed.record.title_source), (Some("Login bug"), TitleSource::ByHand));
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

/// A record reads its title tidied, so that's the title a new one is asked over, however it's stored.
#[test]
fn a_regenerated_title_replaces_one_stored_before_titles_were_tidied() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let untidy = "UPDATE workspaces SET title = 'Login  bug ', title_source = 'by_hand' WHERE id = 'w'";
    lock(&store.writer).execute(untidy, []).unwrap();
    let read = store.get_workspace(&"w".into()).unwrap().title.unwrap();

    let over = regenerated(Some(&read), TitleSource::ByHand);
    let new = store.retitle_workspace(&"w".into(), &title("Fix login redirect"), over).unwrap();

    assert_eq!(read.as_str(), "Login bug");
    assert!(new.changed);
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

/// A workspace from before titles takes its oldest chat's, so whatever is said in it next doesn't become its title.
#[test]
fn a_workspace_from_before_titles_takes_its_oldest_chats() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
            VALUES ('w2', 'r', 'birch', 'birch', 'ready', 'local', 'w2', 0);
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms,
                              updated_at_ms, title)
            VALUES ('later', 'w', 'claude', 'a', 'idle', 0, 2, 2, 'Write the docs'),
                   ('first', 'w', 'claude', 'b', 'idle', 0, 1, 1, 'Fix the login'),
                   ('unprompted', 'w', 'claude', 'c', 'idle', 0, 0, 0, NULL),
                   ('other', 'w2', 'claude', 'd', 'idle', 0, 0, 0, NULL);
    ";

    let store = older::store_from_before(dir.path(), schema::BEFORE_TITLES, rows);

    let workspace = store.get_workspace(&"w".into()).unwrap();
    assert_eq!((workspace.title.as_deref(), workspace.title_source), (Some("Fix the login"), TitleSource::FirstLine));
    assert_eq!(store.get_workspace(&"w2".into()).unwrap().title, None, "no chat has a title, so it has none yet");
}
