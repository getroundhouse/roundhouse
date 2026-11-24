//! An app of your own whose owner the hub doesn't know, as on the first start after the hub stopped keeping what GitHub
//! says of the app in its database, until it has asked GitHub as the app.

use super::super::*;
use super::apps::{APP, ENDS_AT_MS, bot, code, installed, kept, made, on_adalabs};
use super::logins_in;

/// Which installation is on the app's owner can't be told, so the hub connects nothing, as by the clock or as the
/// person asks, and forgets nothing, though GitHub lists no installation at all, until it knows the owner again.
#[test]
fn an_app_whose_owner_isnt_known_has_nothing_connected_and_nothing_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    let account = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);
    logins.observations.forget_for_a_test::<crate::hub::GithubAppSaid>(&APP);
    let token = InstallationToken { token: GithubToken::new("ghs_another"), token_ends_at_ms: ENDS_AT_MS };

    let none_listed = logins.installations_found(APP, &[], false, 2000).unwrap();
    let asked = logins.installations_found(APP, &[on_adalabs(71)], true, 2000).unwrap();
    let connected = logins.connect_installation(APP, &bot(), &on_adalabs(71), token, true, 2000).unwrap();

    assert!(none_listed.forgot.is_empty() && none_listed.connect.is_empty(), "{none_listed:?}");
    assert!(asked.forgot.is_empty() && asked.connect.is_empty(), "{asked:?}");
    assert_eq!(connected, None);
    let still = logins.store.github_account(&account.id).unwrap().unwrap();
    assert!(still.held_login.is_some(), "its login kept");
    assert!(!logins.keep_app(&made(), &code()).unwrap().changed, "the app as it was");
}
