//! What a hub that starts makes of what one that stopped partway left.

use protocol::model::ToolSignInStatus;

use super::sign_ins::{signed_in, status_of};
use super::support::{KEY_FILE, files_in, path, servers_in, with_key, with_neither};
use crate::login_files::LoginFile;

/// A server with neither, one whose key is there, and one whose key is in a form only a newer build reads are kept,
/// with their files. One whose key is gone, as removing it leaves it when the hub stops between, goes, and so does a
/// file no server names, as adding one leaves it.
#[test]
fn tidying_removes_a_server_whose_key_is_gone_and_a_file_no_server_names_and_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let kept = servers.add(with_key("k", "Linear", "Bearer made-up")).unwrap().server;
    let neither = servers.add(with_neither("n", "Docs")).unwrap().server;
    let newer = servers.add(with_key("newer", "Sentry", "a-key")).unwrap().server;
    let gone = servers.add(with_key("gone", "Figma", "a-key")).unwrap().server;
    let newer_form = r#"{"format":"made_up_by_a_newer_build","made_up":1}"#;
    std::fs::write(path(dir.path(), &store, &newer.id), newer_form).unwrap();
    std::fs::remove_file(path(dir.path(), &store, &gone.id)).unwrap();
    std::fs::write(dir.path().join("tool-servers").join(LoginFile::new().as_str()), KEY_FILE).unwrap();

    servers.tidy().unwrap();

    assert_eq!(store.list_records().unwrap().tool_servers, [neither, kept, newer]);
    assert_eq!(files_in(dir.path()), [KEY_FILE, newer_form]);
}

/// As a hub that stopped once it had written a sign-in's file, and not its server's record, left them: one signed in to
/// again, and one whose sign-in is gone.
#[test]
fn tidying_has_a_sign_ins_record_say_what_its_file_does() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let works = signed_in(&servers, "works", "Linear");
    let gone = signed_in(&servers, "gone", "Docs");
    let same = signed_in(&servers, "same", "Sentry");
    let file = path(dir.path(), &store, &gone.id);
    let kept = std::fs::read_to_string(&file).unwrap();
    let (rest, _tokens) = kept.split_once(r#""tokens":"#).unwrap();
    std::fs::write(file, format!(r#"{rest}"tokens":null}}"#)).unwrap();
    servers.say(&works.id, ToolSignInStatus::NeedsSignIn).unwrap();

    servers.tidy().unwrap();

    assert_eq!(status_of(&store, "works"), ToolSignInStatus::Works);
    assert_eq!(status_of(&store, "gone"), ToolSignInStatus::NeedsSignIn);
    assert_eq!(store.tool_server(&same.id).unwrap().unwrap().0, same, "one that agrees is left as it is");
}

/// What an earlier build finds of a newer one's, and what a file cut short leaves.
#[test]
fn tidying_keeps_a_sign_in_in_a_form_only_a_newer_build_reads_and_removes_one_whose_file_isnt_whole() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let newer = signed_in(&servers, "newer", "Linear");
    let cut = signed_in(&servers, "cut", "Docs");
    let newer_form = r#"{"format":"sign_in_of_a_newer_build"}"#;
    std::fs::write(path(dir.path(), &store, &newer.id), newer_form).unwrap();
    std::fs::write(path(dir.path(), &store, &cut.id), r#"{"format":"sign_in","made_by":"i","#).unwrap();

    servers.tidy().unwrap();

    assert_eq!(store.list_records().unwrap().tool_servers, [newer]);
    assert_eq!(files_in(dir.path()), [newer_form]);
}
