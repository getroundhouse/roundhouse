//! When Connect account's panel offers `gh`'s login on the hub's machine, as the hub said who `gh` is.

use client_core::records::HubRecords;
use protocol::model::{GhLogin, GhLoginWorks, GithubAccount, GithubLoginKind, Records};

use super::super::gh_choice::ChoosingPanel;
use super::made_up::{held, named};

fn offered(gh: Option<GhLogin>, accounts: &[GithubAccount]) -> Option<String> {
    let mut panel = ChoosingPanel::default();
    panel.gh = gh;
    let mut held = HubRecords::default();
    held.apply_list(Records { github_accounts: accounts.to_vec(), ..Records::default() });
    panel.offered(&held.snapshot()).map(ToString::to_string)
}

fn logged_in(name: &str, works: GhLoginWorks) -> GhLogin {
    GhLogin::LoggedIn { username: name.into(), works }
}

#[test]
fn gh_s_login_is_offered_by_its_username_while_github_hasnt_refused_it_and_no_account_holds_it() {
    let held_by = |how| GithubAccount { held_login: Some(held(how, None, false)), ..named("g1", "Ada", false) };

    let works = offered(Some(logged_in("ada", GhLoginWorks::Yes)), &[]);
    let couldnt_tell = offered(Some(logged_in("ada", GhLoginWorks::CouldntTell)), &[]);
    let refused = offered(Some(logged_in("ada", GhLoginWorks::Refused)), &[]);
    let logged_out = offered(Some(GhLogin::LoggedOut), &[]);
    let not_said = offered(None, &[]);
    let held_as_gh = offered(Some(logged_in("ada", GhLoginWorks::Yes)), &[held_by(GithubLoginKind::Gh)]);
    let held_signed_in = offered(Some(logged_in("ada", GhLoginWorks::Yes)), &[held_by(GithubLoginKind::SignIn)]);
    let held_for_another = offered(Some(logged_in("grace", GhLoginWorks::Yes)), &[held_by(GithubLoginKind::Gh)]);

    assert_eq!(works.as_deref(), Some("ada"));
    assert_eq!(couldnt_tell.as_deref(), Some("ada"), "a hub that couldn't ask GitHub refuses as it's used, if it must");
    assert_eq!((refused, logged_out, not_said), (None, None, None));
    assert_eq!(held_as_gh, None, "GitHub's usernames are the same in any case");
    assert_eq!(held_signed_in.as_deref(), Some("ada"), "ada's account can take gh's login in place of its sign-in");
    assert_eq!(held_for_another.as_deref(), Some("grace"));
}
