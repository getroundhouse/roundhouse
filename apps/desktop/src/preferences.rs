//! The app's preferences, as `client_core::this_device` keeps them. They aren't settings, since they change as the app is
//! used, when nothing that follows the settings has anything to do, and nobody has to be told they couldn't be saved.

use crate::saving::saved_before_quitting;
use crate::window_place::Opens;
use client_core::this_device::Preferences;
use gpui_kit::{App, Global};
use protocol::model::WorkspaceId;
use protocol::pairing::Fingerprint;
use std::path::Path;

struct Kept(Preferences);

impl Global for Kept {}

/// Reads what's saved in `data_dir`, once, before the window opens, and has the app wait for what changes to be saved as
/// it quits. Does nothing for a window opened [`Opens::InItsUsualPlace`], for which nothing is kept.
pub(crate) fn load(opens: Opens, data_dir: &Path, cx: &mut App) {
    if opens == Opens::InItsUsualPlace || cx.has_global::<Kept>() {
        return;
    }
    let failed = |e| eprintln!("couldn't keep the app's preferences: {e}");
    cx.set_global(Kept(Preferences::in_dir(data_dir, failed)));
    saved_before_quitting(|Kept(preferences)| preferences.saved(), cx);
}

/// The workspace the app was left on, on the paired hub that's `hub` or on this machine's, if `can_show` says it can
/// still show it.
pub(crate) fn workspace_to_open(
    hub: Option<&Fingerprint>,
    can_show: impl FnOnce(&WorkspaceId) -> bool,
    cx: &App,
) -> Option<WorkspaceId> {
    cx.try_global::<Kept>()?.0.workspace_to_open(hub, can_show).cloned()
}

/// Keeps `workspace` as the one the app is on, on the paired hub that's `hub` or on this machine's.
pub(crate) fn left_on(hub: Option<&Fingerprint>, workspace: &WorkspaceId, cx: &mut App) {
    if cx.has_global::<Kept>() {
        cx.global_mut::<Kept>().0.left_on(hub, Some(workspace));
    }
}
