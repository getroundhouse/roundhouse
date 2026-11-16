//! A local hub makes its own short-lived pairing link. The dialog holds only the displayed copy and request state.

use client_core::Client;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, IntoElement, ParentElement as _, Render, Styled as _, Task,
    WeakEntity, Window, div,
};
use look::{Hand as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::pairing::Pairing;

use super::AppView;

enum State {
    Creating,
    Ready { pairing: Pairing, copied: bool },
    Expired,
    Failed(String),
}

struct ShareHub {
    source: WeakEntity<AppView>,
    state: State,
    work: Option<Task<()>>,
}

pub(crate) fn open(view: WeakEntity<AppView>, window: &mut Window, cx: &mut App) {
    if local_client(&view, cx).is_none() {
        return;
    }
    let sharing = cx.new(|cx| {
        let mut sharing = ShareHub { source: view, state: State::Creating, work: None };
        sharing.generate(cx);
        sharing
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title("Link another device to this hub").child(sharing.clone()).footer(
            DialogFooter::new().child(
                DialogClose::new().trigger(|button| button.sized(look::ControlSize::Large).label("Done").hand()),
            ),
        )
    });
}

fn local_client(view: &WeakEntity<AppView>, cx: &App) -> Option<Client> {
    let view = view.upgrade()?;
    let app = view.read(cx);
    // Pairing is local administration. Never send this request on a saved remote connection.
    app.client().filter(|_| app.hub.remote.is_none())
}

impl ShareHub {
    fn generate(&mut self, cx: &mut Context<Self>) {
        self.work = None;
        let Some(client) = local_client(&self.source, cx) else {
            self.state = State::Failed("This hub is disconnected. Try again once it reconnects.".into());
            cx.notify();
            return;
        };
        self.state = State::Creating;
        let creating = client.start(Client::pairing_start);
        self.work = Some(cx.spawn(async move |this, cx| {
            let result = creating.recv().await;
            let wait_ms = this.update(cx, |this, cx| {
                let wait_ms = match result {
                    Ok(pairing) => {
                        let wait_ms = pairing.expires_at_ms.saturating_sub(look::now_ms(cx)).max(0).unsigned_abs();
                        this.state = State::Ready { pairing, copied: false };
                        Some(wait_ms)
                    }
                    Err(error) => {
                        this.state = State::Failed(format!("Couldn’t create a pairing link: {error}"));
                        None
                    }
                };
                cx.notify();
                wait_ms
            });
            if let Ok(Some(wait_ms)) = wait_ms {
                cx.background_executor().timer(std::time::Duration::from_millis(wait_ms)).await;
                let _ = this.update(cx, |this, cx| {
                    this.state = State::Expired;
                    cx.notify();
                });
            }
        }));
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let State::Ready { pairing, copied } = &mut self.state else { return };
        // A sleeping machine can resume before the timer runs. Never copy a link already known to be expired.
        if pairing.expires_at_ms <= look::now_ms(cx) {
            self.state = State::Expired;
        } else {
            cx.write_to_clipboard(ClipboardItem::new_string(pairing.link.to_string()));
            *copied = true;
        }
        cx.notify();
    }
}

impl Render for ShareHub {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = v_flex().gap(look::Space::Medium)
            .child("On the other device, open the hub switcher, choose Connect another hub…, and paste this link. Both machines must be reachable on the same network or VPN.");
        match &self.state {
            State::Creating => content.child("Creating pairing link…"),
            State::Ready { pairing, copied } => content
                .child(div().px(look::Space::Small).py(look::Space::Small)
                    .bg(look::colors(cx).inset).corners(look::Corner::Control)
                    .typeset(look::Type::Code).truncate().child(pairing.link.to_string()))
                .child("This link works once and expires after ten minutes. Share it only with a device you want to control this hub.")
                .child(h_flex().gap(look::Space::Small)
                    .child(Button::new("copy-pairing-link").primary().sized(look::ControlSize::Large).hand()
                        .label(if *copied { "Copied" } else { "Copy pairing link" })
                        .on_click(cx.listener(|this, _, _, cx| this.copy(cx))))
                    .child(Button::new("new-pairing-link").sized(look::ControlSize::Large).hand().label("Create new link")
                        .on_click(cx.listener(|this, _, _, cx| this.generate(cx))))),
            State::Expired => content.child("This pairing link has expired.").child(retry(cx)),
            State::Failed(error) => content.child(error.clone()).child(retry(cx)),
        }
    }
}

fn retry(cx: &mut Context<ShareHub>) -> impl IntoElement {
    Button::new("retry-pairing-link")
        .primary()
        .sized(look::ControlSize::Large)
        .hand()
        .label("Create new link")
        .on_click(cx.listener(|this, _, _, cx| this.generate(cx)))
}
