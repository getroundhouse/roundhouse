//! Which sign-ins a check finds due, and what renewing one comes to, as its authorization server answered. When, and what
//! is asked of it, is tested with the hub's check, in `crate::hub`.

use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::model::ToolSignInStatus;
use protocol::worker::TokenNumber;
use tool_sign_in::{Granted, OauthError, Renewal};

use super::super::renewals::RENEWED_WITH_LEFT;
use super::super::{Checked, Due, ToolServerChange, ToolServers};
use super::sign_ins::{GIVEN_AT_MS, LASTS, again, granted, signed_in, signed_in_for, status_of};
use super::support::{files_in, holding, path, servers_in, with_key};
use crate::store::later_ms;

/// When each access token here ends, where that's said.
fn ends_at_ms() -> i64 {
    later_ms(GIVEN_AT_MS, LASTS)
}

/// The last moment one that ends then isn't due.
fn not_due_at_ms() -> i64 {
    ends_at_ms() - i64::try_from(RENEWED_WITH_LEFT.as_millis()).unwrap()
}

/// The one renewal a check finds due.
fn only_due(servers: &ToolServers) -> Due {
    let mut due = servers.check_sign_ins(not_due_at_ms() + 1).unwrap().due;
    assert_eq!(due.len(), 1, "{due:?}");
    due.remove(0)
}

fn refused_for_good() -> Renewal {
    Renewal::SignInAgain(OauthError::new("invalid_grant"))
}

/// Beside a key, a token nobody said the end of, one that nothing renews, and one whose sign-in is gone.
#[test]
fn a_sign_in_is_due_once_its_token_has_little_left_if_something_renews_it_and_is_gone_at_its_end_if_nothing_does() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "gone", "Notion");
    servers.renewed(&only_due(&servers), refused_for_good(), 0).unwrap();
    signed_in(&servers, "due", "Linear");
    servers.add(with_key("key", "Figma", "made-up")).unwrap();
    signed_in_for(&servers, "endless", "Docs", Granted { lasts: None, ..granted("made-up-endless") });
    signed_in_for(&servers, "last", "Sentry", Granted { refresh: None, ..granted("made-up-last") });
    let found = |now_ms| {
        let Checked { due, changed } = servers.check_sign_ins(now_ms).unwrap();
        let due: Vec<String> = due.into_iter().map(|due| due.server.to_string()).collect();
        let changed: Vec<String> = changed.into_iter().map(|server| server.id.to_string()).collect();
        (due, changed)
    };
    let of = |id: &str| vec![id.to_string()];

    assert_eq!(found(not_due_at_ms()), (vec![], vec![]));
    assert_eq!(found(not_due_at_ms() + 1), (of("due"), vec![]));
    assert_eq!(found(ends_at_ms() - 1), (of("due"), vec![]));
    assert_eq!(found(ends_at_ms()), (of("due"), of("last")), "one that has ended is still renewed");
    assert_eq!(found(ends_at_ms()), (of("due"), vec![]), "and one that nothing renews is gone once");
    assert_eq!(status_of(&store, "last"), ToolSignInStatus::NeedsSignIn);
    assert_eq!(holding(dir.path(), "made-up-last"), Vec::<String>::new(), "its token is forgotten");
    assert_eq!(status_of(&store, "endless"), ToolSignInStatus::Works);
}

/// The token the hub has, and no other of it, whether or not its end was said, and a server's that has no sign-in.
#[test]
fn a_token_its_server_refused_is_due_if_the_hub_has_it_and_something_renews_it_and_is_gone_if_nothing_does() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in_for(&servers, "t", "Docs", Granted { lasts: None, ..granted("made-up-t") });
    signed_in_for(&servers, "last", "Sentry", Granted { refresh: None, ..granted("made-up-last") });
    servers.add(with_key("key", "Figma", "made-up")).unwrap();
    let refused = |id: &str, number| {
        let Checked { due, changed } = servers.refused(&id.into(), number).unwrap();
        let of = |id: protocol::model::ToolServerId| id.to_string();
        let due: Vec<_> = due.into_iter().map(|due| of(due.server)).collect();
        let changed: Vec<_> = changed.into_iter().map(|server| of(server.id)).collect();
        (due, changed)
    };
    let (nothing, first) = ((vec![], vec![]), TokenNumber::FIRST);

    assert_eq!(refused("t", first.next()), nothing, "it was given no such token");
    assert_eq!(refused("t", first), (vec!["t".to_string()], vec![]));
    assert_eq!(refused("last", first), (vec![], vec!["last".to_string()]));
    assert_eq!(refused("last", first), nothing, "it's gone once");
    assert_eq!((refused("key", first), refused("none", first)), (nothing.clone(), nothing.clone()));
    assert_eq!(status_of(&store, "last"), ToolSignInStatus::NeedsSignIn);
    assert_eq!(holding(dir.path(), "made-up-last"), Vec::<String>::new());
    again(&servers, "t").unwrap();
    assert_eq!(refused("t", first), nothing, "the hub has another by now");
    assert_eq!(refused("t", first.next()).0, ["t".to_string()]);
}

#[test]
fn renewed_tokens_take_the_place_of_those_before_and_leave_the_record_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    let server = signed_in(&servers, "t", "Linear");
    let due = only_due(&servers);

    let change = servers.renewed(&due, Renewal::Renewed(granted("made-up-second")), ends_at_ms()).unwrap();

    assert_eq!(change, Some(ToolServerChange { server: server.clone(), changed: false }));
    assert_eq!(store.list_records().unwrap().tool_servers, std::slice::from_ref(&server));
    let kept = files_in(dir.path());
    let ends_at_ms = later_ms(ends_at_ms(), LASTS);
    let tokens = format!(
        r#""number":2,"tokens":{{"access":"made-up-second","ends_at_ms":{ends_at_ms},"refresh":"made-up-second-refresh"}}}}"#
    );
    assert!(kept.len() == 1 && kept[0].ends_with(&tokens), "{kept:?}");
    assert_eq!(holding(dir.path(), "made-up-t"), Vec::<String>::new());
    assert!(servers.check_sign_ins(not_due_at_ms() + 1).unwrap().due.is_empty(), "it isn't due until the new one is");
    assert_eq!(servers.signed_in_by(&"t".into()).unwrap(), Some(server), "it's still the sign-in that made it");
}

#[test]
fn one_refused_for_good_has_its_tokens_forgotten_and_its_record_say_it_needs_signing_in_to() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");

    let due = only_due(&servers);

    let change = servers.renewed(&due, refused_for_good(), 0).unwrap().unwrap();

    assert!(change.changed);
    assert_eq!(status_of(&store, "t"), ToolSignInStatus::NeedsSignIn);
    assert_eq!(holding(dir.path(), "made-up-t"), Vec::<String>::new());
    let to = servers.signing_in(ToolSignInTo::Again { id: "t".into() }).unwrap();
    let asked = to.asked_before(due.renewable.issuer());
    assert_eq!(asked.to_string(), "read", "what it was asked to allow is kept");
}

/// As on a full disk, here with a directory where the file is written beside the one it replaces. The file goes first, so
/// the record never says what it doesn't, and workers have the token that's still there.
#[test]
fn one_whose_file_cant_be_written_leaves_the_file_the_record_and_what_workers_have_as_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let (servers, store) = servers_in(dir.path());
    signed_in(&servers, "t", "Linear");
    let (due, before) = (only_due(&servers), (files_in(dir.path()), store.list_records().unwrap()));
    let sent = servers.sent();
    let file = path(dir.path(), &store, &"t".into());
    let beside = file.with_file_name(format!("{}.partial", file.file_name().unwrap().to_str().unwrap()));
    std::fs::create_dir(&beside).unwrap();

    let refused = servers.renewed(&due, refused_for_good(), 0);
    let renewed = servers.renewed(&due, Renewal::Renewed(granted("made-up-second")), 0);

    std::fs::remove_dir(&beside).unwrap();
    assert!(refused.is_err() && renewed.is_err());
    assert_eq!((files_in(dir.path()), store.list_records().unwrap()), before);
    assert!(!sent.has_changed().unwrap());
}

/// Each answer comes for tokens the server no longer has: it was signed in to again, renewed, its sign-in went, it was
/// removed, or it was removed and one added under its ID.
#[test]
fn what_comes_of_renewing_tokens_a_server_no_longer_has_is_let_go_of() {
    type Meanwhile = fn(&ToolServers, &Due);
    let meanwhile: [Meanwhile; 5] = [
        |servers, _| {
            again(servers, "t").unwrap();
        },
        |servers, due| {
            servers.renewed(due, Renewal::Renewed(granted("made-up-second")), 0).unwrap();
        },
        |servers, due| {
            servers.renewed(due, refused_for_good(), 0).unwrap();
        },
        |servers, _| {
            servers.remove(&"t".into()).unwrap();
        },
        |servers, _| {
            servers.remove(&"t".into()).unwrap();
            signed_in(servers, "t", "Linear");
        },
    ];
    for (which, meanwhile) in meanwhile.into_iter().enumerate() {
        for late in [|| Renewal::Renewed(granted("made-up-late")), refused_for_good] {
            let dir = tempfile::tempdir().unwrap();
            let (servers, store) = servers_in(dir.path());
            signed_in(&servers, "t", "Linear");
            let due = only_due(&servers);
            assert!(servers.holds(&due).unwrap());
            meanwhile(&servers, &due);
            let before = (store.list_records().unwrap(), files_in(dir.path()));

            let held = servers.holds(&due).unwrap();
            let change = servers.renewed(&due, late(), 0).unwrap();

            assert_eq!((held, change), (false, None), "{which}");
            assert_eq!((store.list_records().unwrap(), files_in(dir.path())), before, "{which}");
        }
    }
}
