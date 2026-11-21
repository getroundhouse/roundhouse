//! Switching hubs and pairing devices through the sidebar's hub switcher and its dialogs, against real hubs over
//! loopback TLS.

#![cfg(test)]

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::AgentKind;
use protocol::pairing::{PairingCode, PairingLink};
use protocol::paths::ENV_LISTEN;

use crate::support::{Stack, Ui, hub_environment, sidebar_row};

/// The hub switcher's row that pairs a device, which names the machine as the app does on each system.
const PAIR: &str =
    if cfg!(target_os = "macos") { "Pair a device with this Mac…" } else { "Pair a device with this computer…" };
/// The hub switcher's row that connects another hub.
const CONNECT: &str = "Connect another hub…";

#[gpui_kit::test]
fn pairing_a_device_copies_its_link_and_confirms_when_the_device_connects(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _hub = stack.start_hub_ahead(hub_environment().set(ENV_LISTEN, "127.0.0.1:0"));
    let mut ui = Ui::open(cx, &stack);

    ui.click("hub-switcher");
    ui.click_menu_item(PAIR);
    ui.wait("the pairing link", |ui| ui.has("copy-pairing-link"));

    ui.click("copy-pairing-link");
    let copied = ui.cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text())).unwrap();
    let link = PairingLink::parse(&copied).unwrap();

    let claimed = std::thread::spawn(move || client_core::pair(&link, "Ada’s iPhone").unwrap());
    ui.wait("the dialog to confirm the paired device", |ui| ui.has("paired-device-card") && ui.has("done-share-hub"));
    assert!(ui.shows_label("share-hub-dialog", "Ada’s iPhone, Connected"));
    claimed.join().unwrap();

    ui.click("done-share-hub");
    assert!(!ui.has_dialog());
}

#[gpui_kit::test]
fn connecting_another_hub_previews_it_keeps_errors_inline_and_switches_with_this_computers_name(
    cx: &mut TestAppContext,
) {
    let local = Stack::new();
    let _local_hub = local.start_hub_ahead(hub_environment().set(ENV_LISTEN, "127.0.0.1:0"));
    let local_repo = local.client().repo_add(local.repo().to_str().unwrap()).unwrap();
    let local_ws = local.client().workspace_create(&local_repo.id, None, None, None).unwrap();

    let remote = Stack::new();
    let _remote_hub = remote.start_hub_ahead(hub_environment().set(ENV_LISTEN, "127.0.0.1:0"));
    let mut ui = Ui::open(cx, &local);
    ui.cx.update(|cx| desktop::app::the_device_name_is("Ada’s MacBook Air", cx));

    ui.click("hub-switcher");
    ui.click_menu_item(CONNECT);
    assert!(ui.off("pair-hub"));

    // A malformed string disables Connect and explains what a pairing link looks like inline.
    ui.input("not-a-link");
    ui.frame();
    assert!(ui.off("pair-hub"));
    assert!(ui.has("connect-hub-error"));

    // Replace with a link whose code isn't live on the remote hub: preview resolves this computer's name in the
    // background, and the rejected claim stays inside the dialog (leaving the window's error bar untouched) and
    // selects the text for replacement.
    let pairing = remote.client().pairing_start().unwrap();
    let mut wrong_code = pairing.link.clone();
    wrong_code.code = PairingCode::from("wrongcode1234567");
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            window.dispatch_action(Box::new(gpui_kit::component::input::SelectAll), cx);
        })
        .unwrap();
    ui.input(&wrong_code.to_string());
    ui.wait("the previewed computer name", |ui| ui.has("pair-link-device-name"));
    assert_eq!(
        ui.labelled(&format!(
            "{}, {}, Pairs this computer as Ada’s MacBook Air",
            wrong_code.hub_name,
            wrong_code.hosts.join(" · ")
        ))
        .len(),
        1
    );
    assert!(ui.on("pair-hub"));

    ui.click("pair-hub");
    ui.wait("the inline rejection", |ui| ui.has("connect-hub-error") && ui.on("pair-hub"));
    assert!(ui.has_dialog());
    assert!(ui.view().error.is_none(), "connection failures stay inside the dialog");

    // Because the rejected code selected the input, pasting the live link replaces it straight away.
    ui.input(&pairing.link.to_string());
    ui.press("enter");
    ui.wait("the app to switch to the remote hub", |ui| !ui.has_dialog() && ui.has("first-switch-local"));
    assert!(ui.off("add-project"), "adding a repository is local to that hub's machine");

    let devices = remote.client().device_list().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "Ada’s MacBook Air");

    // An unread reply arriving on the background local hub updates the active remote hub's switcher badge.
    let local_session = local.client().session_ensure(&local_ws.id, AgentKind::Claude).unwrap();
    local.client().session_prompt(&local_session.id, "say:look here", None).unwrap();
    ui.wait("the background hub's unread count on the switcher", |ui| ui.has("hub-switcher-needs-a-look"));

    // Pairing another device with this computer stays available while viewing the remote hub, and Cancel closes it.
    ui.click("hub-switcher");
    ui.click_menu_item(PAIR);
    ui.wait("the local hub's pairing link", |ui| ui.has("copy-pairing-link"));
    ui.click("cancel-share-hub");
    assert!(!ui.has_dialog());

    // Revoking this computer's credential while the remote hub is still listening returns Unauthorized, showing the
    // tailored recovery screen with Pair again… and Switch to This Mac.
    let _still_listening = remote.client().pairing_start().unwrap();
    remote.client().device_revoke(&devices[0].id).unwrap();
    ui.wait("the revoked screen", |ui| ui.has("pair-hub-again") && ui.has("failed-switch-local"));
    ui.click("failed-switch-local");
    ui.wait("back on this computer's hub", |ui| ui.has(sidebar_row(&local_ws.id)));
}

#[gpui_kit::test]
fn an_open_workspace_on_a_remote_hub_stays_drawn_when_the_hub_goes_unreachable(cx: &mut TestAppContext) {
    let local = Stack::new();
    let _local_hub = local.start_hub_ahead(hub_environment().set(ENV_LISTEN, "127.0.0.1:0"));
    let remote = Stack::new();
    let remote_hub = remote.start_hub_ahead(hub_environment().set(ENV_LISTEN, "127.0.0.1:0"));
    let repo = remote.client().repo_add(remote.repo().to_str().unwrap()).unwrap();
    let ws = remote.client().workspace_create(&repo.id, None, None, None).unwrap();
    let pairing = remote.client().pairing_start().unwrap();

    let mut ui = Ui::open(cx, &local);
    ui.click("hub-switcher");
    ui.click_menu_item(CONNECT);
    ui.input(&pairing.link.to_string());
    ui.press("enter");
    ui.wait("the remote workspace row", |ui| !ui.has_dialog() && ui.has(sidebar_row(&ws.id)));

    ui.click(sidebar_row(&ws.id));
    ui.wait("the remote workspace's composer", |ui| ui.has(look::Region::Composer.id()));

    // Stopping the remote hub makes it unreachable; the open workspace stays on screen across background retries.
    drop(remote_hub);
    ui.wait("the switcher to report the hub offline", |ui| {
        ui.shows_label("hub-switcher", &format!("Switch hub: {}, Offline", pairing.link.hub_name))
    });
    assert!(ui.has(look::Region::Composer.id()), "the open workspace stays on screen while unreachable");
}
