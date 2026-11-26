//! What's kept of what a sign-in was granted, which signing in again asks for too, and what it changes of the rules for
//! what a server wants, which is nothing but what's asked for.

use protocol::model::{ToolScopes, ToolSignInStatus};
use protocol::worker::{ServerSignIn, ToolAccess};
use tool_sign_in::{Granted, OauthError, Renewal};

use super::super::ToolServers;
use super::sign_ins::{FILE, GIVEN_AT_MS, given, granted, new, signed_in, status_of};
use super::support::{files_in, holding, path, servers_in};
use super::wanted::{FIRST, ISSUER, asks, starts, wants};

/// What an authorization server gives, with the access token `access`, and says it's allowed `said`, or doesn't say.
fn allowed(access: &str, said: Option<&str>) -> Granted {
    let scopes = said.map(|said| ToolScopes::from(said.to_string()));
    Granted { scopes, ..granted(access) }
}

/// Signs in to the new server `t`, asked to allow `asked`, and given `granted`.
fn first(servers: &ToolServers, asked: &str, granted: Granted) {
    let to = servers.signing_in(new("t", "Linear")).unwrap();
    servers.keep_sign_in("t".into(), &to, given(asked, ISSUER, granted), GIVEN_AT_MS).unwrap();
}

/// Signs in to it again, asked to allow what that asks for, with the answer saying `said`.
fn again(servers: &ToolServers, said: Option<&str>) {
    let to = starts(servers);
    let signed_in = given(&asks(&to, ISSUER), ISSUER, allowed("made-up-again", said));
    servers.keep_sign_in("again".into(), &to, signed_in, GIVEN_AT_MS).unwrap();
}

/// Renews its sign-in, with `renewal` what that comes to.
fn renews(servers: &ToolServers, renewal: Renewal) {
    let mut due = servers.check_sign_ins(i64::MAX).unwrap().due;
    servers.renewed(&due.remove(0), renewal, GIVEN_AT_MS).unwrap().unwrap();
}

fn asked_again_for(servers: &ToolServers) -> String {
    asks(&starts(servers), ISSUER)
}

/// As [`FILE`]'s, whose answer says nothing, with what this one's does.
#[test]
fn what_was_granted_is_kept_beside_its_sign_in_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());

    first(&servers, "read", allowed("made-up-t", Some("made-up:default read")));

    let kept = format!("{},{}", FILE.strip_suffix('}').unwrap(), r#""granted":"made-up:default read"}"#);
    assert_eq!(files_in(dir.path()), [kept]);
    assert_eq!(holding(dir.path(), "made-up:default"), ["tool-servers"], "nothing clients are told of has it");
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works);
    let works = Some(ServerSignIn::Works { token: FIRST });
    assert_eq!(servers.sent_server(&"t".into()).unwrap().sign_in, works);
}

/// As one from a build that kept nothing of what was granted.
#[test]
fn a_file_that_says_nothing_of_what_was_granted_reads_with_none() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    std::fs::write(path(dir.path(), &store, &"t".into()), FILE).unwrap();

    assert_eq!(asked_again_for(&servers), "read");
    renews(&servers, Renewal::Renewed(allowed("made-up-renewed", Some("read admin"))));
    assert_eq!(asked_again_for(&servers), "read admin");
}

/// Such as what its authorization server grants when it's asked for nothing.
#[test]
fn signing_in_again_asks_for_what_was_asked_for_what_was_granted_and_what_is_wanted_each_once_and_only_there() {
    let cases = [
        ("", Some("read write"), "read write admin"),
        ("read", Some("write read admin"), "read write admin"),
        ("read write", Some("read"), "read write admin"),
        ("read", None, "read admin"),
        ("", None, "admin"),
    ];
    for (asked, said, asks_for) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (servers, _store) = servers_in(dir.path());
        first(&servers, asked, allowed("made-up-t", said));
        wants(&servers, FIRST, "admin").change.unwrap();

        assert_eq!(asked_again_for(&servers), asks_for, "{asked:?}, {said:?}");
        assert_eq!(asks(&starts(&servers), "https://another.example.com"), "", "{asked:?}, {said:?}");
    }
}

#[test]
fn a_renewal_that_says_what_it_granted_takes_the_place_of_what_was_and_one_that_doesnt_granted_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    first(&servers, "read", allowed("made-up-t", Some("read write admin")));
    let after = |renewal| {
        renews(&servers, renewal);
        asked_again_for(&servers)
    };

    let silent = after(Renewal::Renewed(allowed("made-up-2", None)));
    let narrowed = after(Renewal::Renewed(allowed("made-up-3", Some("write"))));
    let silent_again = after(Renewal::Renewed(allowed("made-up-4", None)));
    let gone = after(Renewal::SignInAgain(OauthError::new("invalid_grant")));

    assert_eq!(silent, "read write admin");
    assert_eq!(narrowed, "read write", "what was asked for at first is still");
    assert_eq!((silent_again.as_str(), gone.as_str()), ("read write", "read write"));
}

/// One sign-in's answer says nothing of another's.
#[test]
fn signing_in_again_keeps_what_its_own_answer_says_and_has_asked_for_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    first(&servers, "", allowed("made-up-t", Some("read write")));

    again(&servers, None);

    let kept = files_in(dir.path()).remove(0);
    assert!(kept.contains(r#""scopes":"read write""#) && !kept.contains("granted"), "{kept}");
}

/// As when the person leaves it out on the page. It's asked for once more, and then no more, though each answer grants
/// something else that nobody asked for, which signing in again then asks for too.
#[test]
fn a_scope_asked_for_and_not_granted_is_asked_for_once_more_and_then_no_more_whatever_else_is_granted() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    first(&servers, "read write", allowed("made-up-t", Some("read other:1")));

    wants(&servers, FIRST, "write").change.unwrap();
    assert_eq!(asked_again_for(&servers), "read write other:1");
    again(&servers, Some("read other:2"));

    assert_eq!(wants(&servers, FIRST.next(), "write").access, ToolAccess::NotAskedFor);
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::Works);
}

/// A server may not take what it was granted as enough, and it was never asked for by name, which the person may then
/// leave out. So it's asked for, then once more, as what's neither asked for nor granted is.
#[test]
fn a_scope_granted_without_being_asked_for_is_asked_for_by_name_then_once_more_and_then_no_more() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    let says = Some("read write");
    first(&servers, "", allowed("made-up-t", says));
    let mut number = FIRST;

    for _ in 0..2 {
        assert_eq!(wants(&servers, number, "write").access, ToolAccess::SignInAgain);
        again(&servers, says);
        number = number.next();
    }

    assert_eq!(wants(&servers, number, "write").access, ToolAccess::NotAskedFor);
}

/// Once each was asked for again in vain, only what hasn't been asked for is asked for, though it's been granted.
#[test]
fn what_is_granted_is_never_in_vain_until_its_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, _store) = servers_in(dir.path());
    first(&servers, "read", allowed("made-up-t", None));
    wants(&servers, FIRST, "read").change.unwrap();
    again(&servers, Some("read extra"));

    assert_eq!(wants(&servers, FIRST.next(), "read").access, ToolAccess::NotAskedFor);
    assert_eq!(wants(&servers, FIRST.next(), "extra").access, ToolAccess::SignInAgain);
}

/// It was wanted after this sign-in was asked for.
#[test]
fn what_is_wanted_as_its_signed_in_to_again_is_still_to_ask_for_unless_its_granted_all_the_same() {
    for (said, status) in [
        (None, ToolSignInStatus::NeedsMoreAccess),
        (Some("read"), ToolSignInStatus::NeedsMoreAccess),
        (Some("read late"), ToolSignInStatus::Works),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (servers, store) = servers_in(dir.path());
        first(&servers, "read", allowed("made-up-t", None));
        let to = starts(&servers);

        wants(&servers, FIRST, "late").change.unwrap();
        let signed_in = given(&asks(&to, ISSUER), ISSUER, allowed("made-up-again", said));
        servers.keep_sign_in("again".into(), &to, signed_in, GIVEN_AT_MS).unwrap();

        assert_eq!(status_of(&store, "t"), status, "{said:?}");
    }
}
