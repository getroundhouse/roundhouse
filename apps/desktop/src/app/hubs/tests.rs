use super::*;
use crate::app::{AppView, HubSource};
use client_core::RemoteHub;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AnyWindowHandle, TestAppContext};
use gpui_kit::{AppContext as _, Entity};
use protocol::pairing::DeviceToken;
use protocol::pairing::Fingerprint;

pub(in crate::app) fn remote(device: &str) -> RemoteHub {
    RemoteHub {
        hub_name: "Test Mac".into(),
        hosts: vec!["127.0.0.1:1".into()],
        fingerprint: Fingerprint::of_sha256([1; 32]),
        device_id: device.into(),
        token: DeviceToken::from(device),
    }
}

/// The real host and views in a test window. Connections are cancelled before the scheduler starts them; these
/// tests exercise window ownership, while the loopback network tests exercise the hub and draft keeper.
pub(in crate::app) fn host(cx: &mut TestAppContext) -> (tempfile::TempDir, AnyWindowHandle, Entity<Hubs>) {
    let dir = tempfile::tempdir().unwrap();
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(dir.path().join("data").display().to_string()),
        protocol::paths::ENV_WORKSPACES => Some(dir.path().join("workspaces").display().to_string()),
        _ => None,
    })
    .unwrap();
    let source = HubSource {
        remote: None,
        daemon: client_core::Daemon::in_place(dir.path().join("unused")),
        paths,
        build_number: None,
        build_version: "test",
    };
    let (window, host) = cx.update(|cx| {
        gpui_kit::init(cx);
        look::apply(cx);
        dictation_ui::install(
            dictation_ui::Source {
                ready: crate::dictating::ready,
                record: crate::dictating::record,
                uses_microphone: crate::dictating::hears_the_microphone,
            },
            cx,
        );
        crate::dictating::start(&source.paths.data_dir, cx);
        let (window, host) = gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Hubs::new(source, None, window, cx))
        })
        .unwrap();
        host.read(cx).active().update(cx, |view, _| view.connecting = None);
        (window, host)
    });
    (dir, window, host)
}

pub(in crate::app) fn pick(
    remote: Option<RemoteHub>,
    window: AnyWindowHandle,
    host: &Entity<Hubs>,
    cx: &mut TestAppContext,
) -> Entity<AppView> {
    cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                select(remote, window, cx);
                let view = host.read(cx).active();
                view.update(cx, |view, _| view.connecting = None);
                view
            })
            .unwrap()
    })
}

#[gpui_kit::test]
fn re_pairing_reuses_the_view_and_replaces_its_authorization(cx: &mut TestAppContext) {
    let (_dir, window, host) = host(cx);
    let first = pick(Some(remote("revoked")), window, &host, cx);
    first.update(cx, |view, _| view.error = Some("unsent work stays here".into()));
    pick(None, window, &host, cx);
    let renewed = pick(Some(remote("renewed")), window, &host, cx);
    assert_eq!(first.entity_id(), renewed.entity_id());
    cx.read(|cx| {
        assert_eq!(host.read(cx).views.len(), 2);
        let view = renewed.read(cx);
        assert_eq!(view.error.as_deref(), Some("unsent work stays here"));
        assert_eq!(view.hub.remote.as_ref().unwrap().token.as_str(), "renewed");
    });
}

#[gpui_kit::test]
fn an_old_credential_read_cannot_override_a_later_local_choice(cx: &mut TestAppContext) {
    let (_dir, window, host) = host(cx);
    let intent = cx.update(begin_selection);
    pick(None, window, &host, cx);
    cx.update(|cx| window.update(cx, |_, window, cx| finish_selection(intent, remote("old"), window, cx)).unwrap());
    cx.read(|cx| assert!(host.read(cx).selected.is_none()));
}

#[gpui_kit::test]
fn hidden_hubs_leave_focus_and_dialogs_with_the_selected_hub(cx: &mut TestAppContext) {
    let (_dir, window, host) = host(cx);
    let local = cx.read(|cx| host.read(cx).active());
    let other = pick(Some(remote("other")), window, &host, cx);
    cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                local.update(cx, |view, cx| {
                    view.focus_what_shows(window, cx);
                    view.present(cx, |_, window, cx| {
                        window.open_dialog(cx, |dialog, _, _| dialog.title("Returned prompt"));
                    });
                });
                assert!(other.read(cx).focus.is_focused(window));
            })
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                assert!(!window.has_active_dialog(cx));
                assert!(other.read(cx).focus.is_focused(window));
            })
            .unwrap();
    });
    pick(None, window, &host, cx);
    cx.update(|cx| window.update(cx, |_, window, cx| assert!(window.has_active_dialog(cx))).unwrap());
}
