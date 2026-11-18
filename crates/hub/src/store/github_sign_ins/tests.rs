use protocol::model::{GithubAccountId, GithubLoginKind, HeldGithubLogin};

use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_GITHUB_SIGN_INS;
use crate::store::tests::open_in;
use crate::store::{GithubApproving, Store};
use github::GithubProfile;

fn waiting() -> GithubSignInStatus {
    protocol::fixtures::github_sign_in("s").status
}

fn ada() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "ada".into() }
}

/// The sign-in `sign_in` approved as `user`, with no name or public email on GitHub, kept in `file`, signed in at 100,
/// its token ending at 900, as the account `new_account` if it's new.
fn approving<'a>(
    sign_in: &'a GithubSignInId,
    user: &'a GithubUser,
    file: &'a LoginFile,
    new_account: &'a GithubAccountId,
) -> GithubApproving<'a> {
    const NONE: &GithubProfile = &GithubProfile { name: None, email: None };
    GithubApproving { sign_in, user, profile: NONE, file, new_account, signed_in_at_ms: 100, ends_at_ms: Some(900) }
}

fn started(store: &Store, id: &str) -> GithubSignInId {
    let id = GithubSignInId::from(id);
    assert!(store.start_github_sign_in(&id, &waiting()).unwrap().made);
    id
}

#[test]
fn a_hub_from_before_sign_ins_has_none_and_the_logins_it_held_need_no_signing_in_again() {
    let dir = tempfile::tempdir().unwrap();
    let held = "INSERT INTO github_accounts
                    (id, user_id, username, login_kind, login_file, login_connected_at_ms, login_ends_at_ms)
                    VALUES ('g', 7, 'ada', 'token', '6f1c2a9e-0000-4000-8000-000000000000', 1, NULL);";

    let store = store_from_before(dir.path(), BEFORE_GITHUB_SIGN_INS, held);

    let listed = store.list_records().unwrap();
    assert_eq!(listed.github_sign_ins, []);
    let held = listed.github_accounts[0].held_login.unwrap();
    assert_eq!(
        held,
        HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms: 1, ends_at_ms: None, needs_sign_in: false }
    );
}

#[test]
fn starting_one_again_finds_it_and_starting_another_removes_those_that_ended_each_with_a_revision_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let [ended, still_waiting] = ["ended", "waiting"].map(|id| started(&store, id));
    let ended_as = store.set_github_sign_in(&ended, &GithubSignInStatus::Refused).unwrap();

    let again = store.start_github_sign_in(&ended, &waiting()).unwrap();
    let another = store.start_github_sign_in(&"another".into(), &waiting()).unwrap();

    assert_eq!((again.sign_in.clone(), again.made, again.removed), (ended_as.clone(), false, Vec::new()));
    let [(removed, removed_at)] = another.removed.as_slice() else { panic!("{:?}", another.removed) };
    assert_eq!(removed, &ended);
    assert!(*removed_at > ended_as.rev, "{removed_at} isn't past {}", ended_as.rev);
    assert!(another.sign_in.rev > *removed_at, "{} isn't past {removed_at}", another.sign_in.rev);
    let listed: Vec<GithubSignInId> = store.list_records().unwrap().github_sign_ins.into_iter().map(|s| s.id).collect();
    assert_eq!(listed, [still_waiting, "another".into()]);
}

#[test]
fn approving_makes_an_account_then_renews_the_one_signed_in_as_the_same_user_in_place_and_one_by_a_token_isnt_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let by_token = GithubAccountId::from("by-token");
    let token_file = LoginFile::new();
    let connecting = crate::store::GithubConnecting {
        account: &by_token,
        user: &ada(),
        how: GithubLoginKind::Token,
        file: &token_file,
        connected_at_ms: 1,
        ends_at_ms: None,
        installation: None,
        profile: None,
    };
    store.connect_github_login(&connecting).unwrap();
    let [first, second] = ["first", "second"].map(|id| started(&store, id));
    let (first_file, second_file) = (LoginFile::new(), LoginFile::new());

    let made = store.approve_github_sign_in(&approving(&first, &ada(), &first_file, &"made".into())).unwrap();
    let renewed = store.approve_github_sign_in(&approving(&second, &ada(), &second_file, &"unused".into())).unwrap();

    assert_eq!(made.sign_in.status, GithubSignInStatus::Approved { account: "made".into() });
    assert_eq!(made.connected.account.held_login.map(|held| held.how), Some(GithubLoginKind::SignIn));
    assert_eq!(made.connected.settings, None, "the account held by a token is the default");
    assert_eq!(renewed.sign_in.status, GithubSignInStatus::Approved { account: "made".into() });
    assert_eq!(renewed.connected.replaced, Some(first_file), "the first sign-in's file is let go of");
    let accounts = store.list_records().unwrap().github_accounts;
    assert_eq!(accounts.iter().map(|account| account.id.as_str()).collect::<Vec<_>>(), ["by-token", "made"]);
}

#[test]
fn setting_a_sign_in_the_hub_has_no_record_of_is_refused_and_starting_the_hub_ends_only_those_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    started(&store, "waiting");
    started(&store, "refused");
    let refused = store.set_github_sign_in(&"refused".into(), &GithubSignInStatus::Refused).unwrap();

    let nobodys = store.set_github_sign_in(&"nobody's".into(), &GithubSignInStatus::Ended);
    store.end_waiting_github_sign_ins().unwrap();

    assert!(matches!(nobodys, Err(crate::store::StoreError::NotFound { .. })), "{nobodys:?}");
    let [ended, kept] = store.list_records().unwrap().github_sign_ins.try_into().unwrap();
    assert_eq!((ended.id.as_str(), ended.status), ("waiting", GithubSignInStatus::Ended));
    let (newest, ended_at) = (refused.rev, ended.rev);
    assert!(ended_at > newest, "{ended_at:?} isn't past {newest:?}, so a client holding it waiting would keep it so");
    assert_eq!(kept, refused, "one that had ended is as it was");
}
