//! Providers' screens in Settings, each of a state its rules in `settings.md` say: this Mac with `gh` and its SSH key in
//! each state, how its pushes go, and a provider whose GitHub Roundhouse sets up.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{
    providers_gh_logged_out, providers_hub_is_a_bot, providers_key_differs, providers_managed, providers_no_gh,
    providers_no_key, providers_ssh,
};
use client_core::fake_hub::World;

/// Shows Providers, once Settings is open.
const PROVIDERS: Step = Step::Click("settings-page-providers");

const fn providers(name: &'static str, world: fn() -> World) -> Screen {
    Screen {
        name,
        spec: "settings.md",
        world: Some(world),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, PROVIDERS],
    }
}

pub(crate) const SCREENS: &[Screen] = &[
    providers("settings-providers", providers_ssh),
    providers("settings-providers-no-key", providers_no_key),
    providers("settings-providers-key-differs", providers_key_differs),
    providers("settings-providers-gh-logged-out", providers_gh_logged_out),
    providers("settings-providers-no-gh", providers_no_gh),
    providers("settings-providers-hub-is-a-bot", providers_hub_is_a_bot),
    providers("settings-providers-managed", providers_managed),
];
