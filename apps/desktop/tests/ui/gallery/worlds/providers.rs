//! Providers in Settings: this Mac, whose GitHub is left to it, with `gh` and an SSH key in each state they can be
//! in, as the hub reads them as the page opens, beside who the hub itself reads GitHub as, and a provider whose GitHub
//! Roundhouse sets up.

use protocol::model::{GhLogin, GhLoginWorks, MachineLogins, SshLogin};

use super::github_app_installed;
use super::github_gh_default;
use client_core::fake_hub::World;

fn ada_logged_in() -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes }
}

fn key_of(username: &str) -> SshLogin {
    SshLogin::Key { username: username.into() }
}

/// `world`, whose hub leaves GitHub to this Mac, and answers that its `gh` and SSH key are as `gh` and `ssh` say.
fn with_this_mac(mut world: World, gh: GhLogin, ssh: SshLogin) -> World {
    let github = world.github.as_mut().expect("a hub that keeps GitHub accounts");
    github.left_to_machine = vec!["local".into()];
    world.machine_logins = Some(MachineLogins { gh: Some(gh), ssh });
    world
}

/// `gh` and the SSH key are both `ada`'s, on a hub that reads GitHub as `gh`'s login, `ada`.
pub(in crate::gallery) fn providers_ssh() -> World {
    with_this_mac(github_gh_default(), ada_logged_in(), key_of("ada"))
}

/// No SSH key that GitHub takes.
pub(in crate::gallery) fn providers_no_key() -> World {
    with_this_mac(github_gh_default(), ada_logged_in(), SshLogin::NoKey)
}

/// `gh` is `ada`, and the SSH key is `grace`'s.
pub(in crate::gallery) fn providers_key_differs() -> World {
    with_this_mac(github_gh_default(), ada_logged_in(), key_of("grace"))
}

/// `gh` is logged out, with `ada`'s SSH key.
pub(in crate::gallery) fn providers_gh_logged_out() -> World {
    with_this_mac(github_gh_default(), GhLogin::LoggedOut, key_of("ada"))
}

/// There's no `gh`, with `ada`'s SSH key.
pub(in crate::gallery) fn providers_no_gh() -> World {
    with_this_mac(github_gh_default(), GhLogin::NotInstalled, key_of("ada"))
}

/// The hub reads GitHub as an app's bot, `ada-roundhouse[bot]`, while this Mac's `gh` and SSH key are `ada`'s.
pub(in crate::gallery) fn providers_hub_is_a_bot() -> World {
    with_this_mac(github_app_installed(), ada_logged_in(), key_of("ada"))
}

/// [`providers_ssh`], with a second provider, `incus`, whose GitHub Roundhouse sets up.
pub(in crate::gallery) fn providers_managed() -> World {
    let mut world = providers_ssh();
    world.github.as_mut().unwrap().set_up = vec!["incus".into()];
    world
}
