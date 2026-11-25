//! The app opening where it was left. `client_core::this_device`'s unit tests have what's kept of that. A screen is known
//! by what only it has, as in `flows.rs`.

#![cfg(test)]

use std::path::Path;
use std::sync::Arc;

use client_core::this_device::Preferences;
use gpui_kit::{ElementId, TestAppContext};
use mobile::{MemoryVault, Vault as _};
use protocol::model::{Workspace, WorkspaceId};
use protocol::pairing::Fingerprint;

use crate::support::{Phone, Stack, TIMEOUT, paired_vault};

fn row(workspace: &Workspace) -> ElementId {
    ElementId::Name(format!("workspace-{}", workspace.id).into())
}

/// Where the app was left, as a phone keeps it in `dir`.
fn kept_in(dir: &Path) -> Preferences {
    Preferences::in_dir(dir, |failure| panic!("{failure}"))
}

/// Where an app left on `workspace` on the hub in `vault` was left, kept in `dir`.
fn left_on(workspace: &Workspace, vault: &MemoryVault, dir: &Path) -> Preferences {
    let mut preferences = kept_in(dir);
    preferences.left_on(Some(&vault.load().unwrap().fingerprint), Some(&workspace.id));
    preferences.saved().recv_timeout(TIMEOUT).unwrap();
    preferences
}

/// Waits until the phone keeps `workspace` as where it was left on `hub`, or the list, with none.
fn wait_until_kept(phone: &mut Phone, hub: &Fingerprint, dir: &Path, workspace: Option<&WorkspaceId>) {
    phone.wait("where it was left to be kept", |_| kept_in(dir).last_workspace(Some(hub)) == workspace);
}

/// Each test app has its own memory, so each time the app opens it has only what was kept.
#[gpui_kit::test]
fn the_app_opens_on_the_chat_it_was_left_on_until_someone_goes_back_to_the_list(
    first: &mut TestAppContext,
    again: &mut TestAppContext,
    after: &mut TestAppContext,
) {
    let (stack, dir) = (Stack::new(), tempfile::tempdir().unwrap());
    let (vault, workspace) = (paired_vault(&stack), stack.workspace());
    let hub = vault.load().unwrap().fingerprint;
    let open = |cx| Phone::open(cx, Arc::clone(&vault), kept_in(dir.path()));
    let mut phone = open(first);
    phone.open_chat(&workspace);
    wait_until_kept(&mut phone, &hub, dir.path(), Some(&workspace.id));

    let mut phone = open(again);
    phone.wait("the chat it was left on", |phone| phone.has("composer") && phone.transcript_shown());
    assert_eq!(phone.chat_title().as_deref(), Some(workspace.label()));

    phone.tap("back");
    let made_since = stack.client().workspace_create(&workspace.repo_id, None, None, None).unwrap();
    phone.wait("the list, with one made since", |phone| phone.has(row(&workspace)) && phone.has(row(&made_since)));
    assert!(!phone.has("composer"), "the records changing doesn't open it again");
    wait_until_kept(&mut phone, &hub, dir.path(), None);

    let mut phone = open(after);
    phone.wait("the list", |phone| phone.is_connected() && phone.has(row(&workspace)));
    assert!(!phone.has("composer"));
}

#[gpui_kit::test]
fn an_app_left_on_a_workspace_archived_since_opens_on_the_list(cx: &mut TestAppContext) {
    let (stack, dir) = (Stack::new(), tempfile::tempdir().unwrap());
    let (vault, workspace) = (paired_vault(&stack), stack.workspace());
    let preferences = left_on(&workspace, &vault, dir.path());
    stack.client().workspace_archive(&workspace.id, false).unwrap();

    let mut phone = Phone::open(cx, vault, preferences);

    phone.wait("the list", |phone| phone.is_connected() && phone.has("hub"));
    assert!(!phone.has("composer"));
}

/// Such as the hub's own screen, to see why it can't be reached.
#[gpui_kit::test]
fn what_someone_opened_before_the_hub_was_reached_stays_open(cx: &mut TestAppContext) {
    let (mut stack, dir) = (Stack::new(), tempfile::tempdir().unwrap());
    let (vault, workspace) = (paired_vault(&stack), stack.workspace());
    let preferences = left_on(&workspace, &vault, dir.path());
    stack.stop_hub();
    let mut phone = Phone::open(cx, vault, preferences);
    phone.tap("hub");

    stack.start_hub();
    phone.wait("the connection", |phone| phone.is_connected());

    assert!(phone.has("forget"), "the hub's own screen");
    phone.tap("back");
    let made_since = stack.client().workspace_create(&workspace.repo_id, None, None, None).unwrap();
    phone.wait("the list, with one made since", |phone| phone.has(row(&made_since)));
    assert!(!phone.has("composer"), "nor does the records changing open the chat once it's put away");
}

/// The phone keeps nothing of a hub it's no longer paired with. The hub is stopped, since the list, which is the way to
/// its screen, isn't where the app was left.
#[gpui_kit::test]
fn forgetting_the_hub_or_replacing_it_forgets_where_the_app_was_left_on_it(
    forgetting: &mut TestAppContext,
    replacing: &mut TestAppContext,
) {
    let (mut stack, other, dir) = (Stack::new(), Stack::new(), tempfile::tempdir().unwrap());
    let (vault, workspace) = (paired_vault(&stack), stack.workspace());
    let replaced = paired_vault(&stack);
    let hub = vault.load().unwrap().fingerprint;
    stack.stop_hub();

    let preferences = left_on(&workspace, &vault, dir.path());
    let mut phone = Phone::open(forgetting, vault, preferences);
    phone.tap("hub");
    phone.tap("forget");
    wait_until_kept(&mut phone, &hub, dir.path(), None);

    let preferences = left_on(&workspace, &replaced, dir.path());
    let mut phone = Phone::open(replacing, replaced, preferences);
    phone.links.send(other.pairing_link()).unwrap();
    phone.wait("the question", |phone| phone.has("confirm-pair"));
    phone.tap("confirm-pair");
    wait_until_kept(&mut phone, &hub, dir.path(), None);
}
