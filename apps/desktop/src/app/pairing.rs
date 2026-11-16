//! Connect another machine using a short-lived link from its hub switcher or local CLI.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{App, AppContext as _, ParentElement as _, WeakEntity, Window};
use look::{Hand as _, Sizing as _};
use protocol::pairing::PairingLink;

use super::{AppView, hubs, saved_hubs};

pub(crate) fn open(view: WeakEntity<AppView>, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("roundhouse://pair?…"));
    let field = input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let (field, view) = (field.clone(), view.clone());
        dialog.title("Connect another hub")
            .child("On the other machine, select its local hub and choose Create pairing link… from the hub switcher. Paste the link here. Both machines must be reachable on the same network or VPN.")
            .child(Input::new(&field))
            .footer(DialogFooter::new()
                .child(DialogClose::new().trigger(|button| button.sized(look::ControlSize::Large).label("Cancel").hand()))
                .child(DialogAction::new().child(Button::new("pair-hub").primary().sized(look::ControlSize::Large).label("Connect"))))
            .on_ok(move |_, window, cx| {
                let link = field.read(cx).value().to_string();
                connect(&link, view.clone(), window, cx);
                true
            })
    });
    input.update(cx, |input, cx| input.focus(window, cx));
}

fn connect(link: &str, view: WeakEntity<AppView>, window: &mut Window, cx: &mut App) {
    let intent = hubs::begin_selection(cx);
    let parsed = PairingLink::parse(link.trim());
    let Some(current) = view.upgrade() else { return };
    let data = current.read(cx).hub.paths.data_dir.clone();
    let handle = window.window_handle();
    current.update(cx, |view, cx| view.show_error("Connecting to the other hub…", cx));
    cx.spawn(async move |cx| {
        let result = async {
            let parsed = parsed?;
            let remote = cx.background_spawn(async move { client_core::pair(&parsed, "Roundhouse desktop") }).await?;
            let key = saved_hubs::credential_key(&data, &remote.device_id);
            saved_hubs::keep(&remote, key, cx).await?;
            Ok::<_, anyhow::Error>(remote)
        }
        .await;
        match result {
            Ok(remote) => {
                let _ = view.update(cx, |view, cx| {
                    view.error = None;
                    cx.notify();
                });
                let _ = handle.update(cx, |_, window, cx| hubs::finish_selection(intent, remote, window, cx));
            }
            Err(error) => {
                let _ = view.update(cx, |view, cx| {
                    view.show_error(
                        format!("Couldn’t connect: {error}. Generate a fresh pairing link to try again."),
                        cx,
                    );
                });
            }
        }
    })
    .detach();
}
