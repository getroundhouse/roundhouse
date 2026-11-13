//! Which login the hub gives a launch, of those it holds.

use super::*;

/// The login each launch at `now_ms` gets, by its account's ID and its token.
fn handed_at(logins: &Logins, now_ms: i64) -> Option<(String, String)> {
    let handed = logins.to_hand(AgentKind::Claude, now_ms).unwrap();
    handed.map(|Handed { account, login }| (account.to_string(), login.token.as_str().to_string()))
}

/// What [`handed_at`] says of the login `connect` named `name`, with `token`.
fn as_connected(name: &str, token: &str) -> (String, String) {
    (format!("claude-login-{name}"), token.to_string())
}

#[test]
fn a_launch_gets_the_login_connected_most_recently_that_hasnt_ended() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&Connect { ends_at_ms: None, ..connect("Lasting", "lasting-token", None) }).unwrap();
    let newer = Connect { connected_at_ms: 1500, ends_at_ms: Some(1800), ..connect("Newer", "newer-token", None) };
    logins.connect(&newer).unwrap();

    assert_eq!(handed_at(&logins, 1700), Some(as_connected("Newer", "newer-token")));
    assert_eq!(handed_at(&logins, 1800), Some(as_connected("Lasting", "lasting-token")), "the newer has ended");
    assert_eq!(
        handed_at(&logins, i64::MAX),
        Some(as_connected("Lasting", "lasting-token")),
        "one with no end never does"
    );
    assert!(logins.to_hand(AgentKind::Unknown, 1700).unwrap().is_none(), "none is another kind's");
}

#[test]
fn a_login_renewed_after_another_was_connected_is_what_launches_get_from_then_on() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let older = logins.connect(&connect("Older", "older-token", None)).unwrap().account.id;
    logins.connect(&Connect { connected_at_ms: 1500, ..connect("Newer", "newer-token", None) }).unwrap();

    logins.connect(&Connect { connected_at_ms: 1600, ..connect("Older", "renewed-token", Some(&older)) }).unwrap();

    assert_eq!(handed_at(&logins, 1700), Some(as_connected("Older", "renewed-token")));
    assert_eq!(handed_at(&logins, 2000), None, "both have ended");
}

#[test]
fn a_launch_under_a_login_whose_token_is_gone_fails_rather_than_running_under_another() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("Older", "older-token", None)).unwrap();
    logins.connect(&Connect { connected_at_ms: 1500, ..connect("Newer", "newer-token", None) }).unwrap();
    let newer = std::fs::read_dir(dir.path().join("logins")).unwrap().map(|entry| entry.unwrap().path());
    let newer = newer.into_iter().find(|path| std::fs::read_to_string(path).unwrap() == "newer-token").unwrap();
    std::fs::remove_file(newer).unwrap();

    let handed = logins.to_hand(AgentKind::Claude, 1700);

    assert_eq!(handed.unwrap_err().code, ErrorCode::Internal);
}
