//! The rule's rows for what the hub keeps of GitHub: its accounts and the sign-ins to them.

use protocol::fixtures;
use protocol::model::{GithubAccountId, GithubLoginKind, GithubSignInStatus, GithubUser, GithubUserId, Rev};

use super::super::{GithubConnecting, Store};
use super::rule::Said;
use crate::login_files::LoginFile;

pub(super) fn account(store: &Store) -> Rev {
    store.github_account(&"g".into()).unwrap().unwrap().rev
}

/// The GitHub account `g`, ada's, connected `how`, which makes it the default.
fn connected(store: &Store, how: GithubLoginKind, ends_at_ms: Option<i64>) {
    let (account, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let (connected_at_ms, installation) = (100, None);
    let connecting = GithubConnecting {
        account: &account,
        user: &user,
        how,
        file: &file,
        connected_at_ms,
        ends_at_ms,
        installation,
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
