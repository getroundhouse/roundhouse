//! The rule's rows for what the hub keeps of GitHub: its accounts, the sign-ins to them, the logins found on each
//! provider's machines, the apps of the person's own, and what each repository's account reaches.

use protocol::client::github_apps::GithubManifestCode;
use protocol::fixtures;
use protocol::model::{
    GhLogin, GithubAccountId, GithubAppId, GithubLoginKind, GithubRepositoryName, GithubSignInStatus, GithubUser,
    GithubUserId, Reach, Rev,
};

use super::super::{GithubAppMaking, GithubConnecting, Store};
use super::rule::Said;
use crate::login_files::LoginFile;

pub(super) fn account(store: &Store) -> Rev {
    store.github_account(&"g".into()).unwrap().unwrap().rev
}

/// The GitHub account `g`, ada's, connected `how`, which makes it the default.
fn connected(store: &Store, how: GithubLoginKind, ends_at_ms: Option<i64>) {
    let (account, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let (connected_at_ms, installation, profile) = (100, None, None);
    let connecting = GithubConnecting {
        account: &account,
        user: &user,
        how,
        file: &file,
        connected_at_ms,
        ends_at_ms,
        installation,
        profile,
    };
    store.connect_github_login(&connecting).unwrap();
}

pub(super) fn by_token(store: &Store) {
    connected(store, GithubLoginKind::Token, None);
}

pub(super) fn signed_in(store: &Store) {
    connected(store, GithubLoginKind::SignIn, Some(9_000));
}

pub(super) fn clear(store: &Store) -> Said {
    Said::Changed(store.clear_github_login(&"g".into()).unwrap().is_some())
}

pub(super) fn needs(store: &Store) -> Said {
    Said::Changed(store.github_login_needs_sign_in(&"g".into()).unwrap().is_some())
}

fn ada() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "ada".into() }
}

pub(super) fn sign_in(store: &Store) -> Rev {
    store.github_sign_in(&"i".into()).unwrap().unwrap().rev
}

pub(super) fn waiting(store: &Store) {
    store.start_github_sign_in(&"i".into(), &fixtures::github_sign_in("i").status).unwrap();
}

pub(super) fn refused(store: &Store) -> Said {
    Said::record(&store.set_github_sign_in(&"i".into(), &GithubSignInStatus::Refused).unwrap())
}

pub(super) fn found_rev(store: &Store) -> Rev {
    store.list_records().unwrap().found_github_logins[0].rev
}

pub(super) fn found(store: &Store) -> Said {
    Said::record(&store.record_found_github_login(&"local".into(), &GhLogin::LoggedOut).unwrap())
}

const APP: GithubAppId = GithubAppId::new(9);

pub(super) fn app(store: &Store) -> Rev {
    store.list_records().unwrap().github_apps[0].rev
}

pub(super) fn made_app(store: &Store) {
    let (key, code, owner) = (LoginFile::new(), GithubManifestCode::new("a-code"), ada());
    let slug = "ada-roundhouse".into();
    let making =
        GithubAppMaking { id: APP, slug: &slug, name: "Ada's Roundhouse", owner: &owner, key: &key, made_from: &code };
    store.make_github_app(&making).unwrap();
}

pub(super) fn bot(store: &Store) -> Said {
    let bot = GithubUser { id: GithubUserId::new(41), name: "ada-roundhouse[bot]".into() };
    Said::Changed(store.set_github_app_bot(APP, &bot).unwrap().is_some())
}

pub(super) fn reach_rev(store: &Store) -> Rev {
    store.github_reach_of(&"r".into()).unwrap().unwrap().rev
}

pub(super) fn reach(store: &Store) -> Said {
    let pushes = Reach::Pushes { repository: GithubRepositoryName::named("cli/cli"), username: "ada".into() };
    Said::record(&store.record_github_reach(&"r".into(), &pushes, Some(5)).unwrap())
}
