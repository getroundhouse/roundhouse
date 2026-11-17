//! Creating a single-use pairing link on this computer's hub, showing its QR code and link, and confirming when a
//! device connects.

use std::path::PathBuf;
use std::time::Duration;

use client_core::{Client, PairingWait};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _,
    Window, div, px,
};
use look::{Corner, IconSize, IconSizing as _, Shaped as _, Space, TaskRing, Type, Typeset as _};
use protocol::pairing::{Device, Pairing};

use super::qr::{self, QrMatrix};
use crate::app::hubs;
use crate::names::MACHINE;

const DIALOG_WIDTH: Pixels = px(512.);

enum State {
    Creating,
    Ready { pairing: Pairing, qr: Option<QrMatrix>, copied: bool },
    Paired { device: Device },
    Expired,
    Failed(String),
}

struct ShareHub {
    state: State,
    work: Option<Task<()>>,
    expiry: Option<Task<()>>,
    waiting: Option<PairingWait>,
}

pub(crate) fn open(window: &mut Window, cx: &mut App) {
    if local_hub(cx).is_none() {
        return;
    }
    let sharing = cx.new(|cx| {
        let mut sharing = ShareHub { state: State::Creating, work: None, expiry: None, waiting: None };
        sharing.generate(cx);
        sharing
    });
    let shown = sharing.clone();
    window.open_dialog(cx, move |surface, _, cx| {
        let colors = look::colors(cx);
        let closing = shown.clone();
        surface
            .p_0()
            .w(DIALOG_WIDTH)
            .close_button(false)
            .bg(colors.raised)
            .border_color(colors.edge)
            .corners(Corner::Card)
            .child(shown.clone())
            .on_ok(|_, _, _| false)
            .on_close(move |_, _, cx| closing.update(cx, ShareHub::closed))
    });
}

fn local_hub(cx: &App) -> Option<(Client, PathBuf)> {
    let view = hubs::local_view(cx)?;
    let app = view.read(cx);
    // Pairing is local administration. Never send this request on a saved remote connection.
    let client = app.client().filter(|_| app.hub.remote.is_none())?;
    Some((client, app.hub.paths.socket.clone()))
}

impl ShareHub {
    fn closed(&mut self, _: &mut Context<Self>) {
        self.work = None;
        self.expiry = None;
        self.waiting = None;
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        self.closed(cx);
        let Some((client, socket)) = local_hub(cx) else {
            self.state = State::Failed("This hub is disconnected. Try again once it reconnects.".into());
            cx.notify();
            return;
        };
        self.state = State::Creating;
        let creating = client.start(Client::pairing_start);
        self.work = Some(cx.spawn(async move |this, cx| {
            let result = creating.recv().await;
            let _ = this.update(cx, |this, cx| this.started(result, socket, cx));
        }));
        cx.notify();
    }

    fn started(&mut self, result: client_core::Result<Pairing>, socket: PathBuf, cx: &mut Context<Self>) {
        let pairing = match result {
            Ok(pairing) => pairing,
            Err(error) => {
                self.state = State::Failed(format!("Couldn’t create a pairing link: {error}"));
                cx.notify();
                return;
            }
        };
        let wait_ms = pairing.expires_at_ms.saturating_sub(look::now_ms(cx)).max(0).unsigned_abs();
        let qr = QrMatrix::of(&pairing.link.to_string());
        let wait = PairingWait::start(socket, pairing.code.clone());
        let paired_rx = wait.recv().clone();
        self.waiting = Some(wait);
        self.state = State::Ready { pairing, qr, copied: false };
        self.work = Some(cx.spawn(async move |this, cx| match paired_rx.recv_async().await {
            Ok(Ok(device)) => {
                let _ = this.update(cx, |this, cx| {
                    this.expiry = None;
                    this.waiting = None;
                    this.state = State::Paired { device };
                    cx.notify();
                });
            }
            Ok(Err(error)) => {
                let _ = this.update(cx, |this, cx| {
                    this.expiry = None;
                    this.waiting = None;
                    this.state = if error.is_not_found() {
                        State::Expired
                    } else {
                        State::Failed(format!("Lost connection while waiting for a device: {error}"))
                    };
                    cx.notify();
                });
            }
            Err(_) => {}
        }));
        self.expiry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(wait_ms)).await;
            let _ = this.update(cx, |this, cx| {
                if matches!(this.state, State::Ready { .. }) {
                    this.work = None;
                    this.waiting = None;
                    this.state = State::Expired;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let State::Ready { pairing, copied, .. } = &mut self.state else { return };
        // A sleeping machine can resume before the timer runs. Never copy a link already known to be expired.
        if pairing.expires_at_ms <= look::now_ms(cx) {
            self.work = None;
            self.waiting = None;
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
        let default_title = format!("Pair a device with this {MACHINE}");
        match &self.state {
            State::Creating => frame(
                default_title,
                Some("Preparing a single-use pairing link…".into()),
                Some(waiting_line("Creating pairing link…", cx)),
                None,
                close_only("Cancel", cx),
                cx,
            ),
            State::Ready { pairing, qr, copied } => {
                let until = look::time_of_day(pairing.expires_at_ms, cx);
                let status = match until {
                    Some(time) => format!("Waiting for a device · Works until {time}"),
                    None => "Waiting for a device".to_string(),
                };
                let body = v_flex()
                    .gap(Space::Large)
                    .child(ready_row(&pairing.link.to_string(), qr.as_ref(), cx))
                    .child(waiting_line(&status, cx))
                    .into_any_element();
                let lead = "Scan on a phone, or choose Connect another hub… on another computer and paste this link. \
                            Both devices must share a network or VPN.";
                frame(default_title, Some(lead.into()), Some(body), None, ready_footer(*copied, cx), cx)
            }
            State::Paired { device } => {
                let title = format!("Paired with {}", device.name);
                let lead =
                    format!("It can open and work in this {MACHINE}’s workspaces whenever this {MACHINE} is awake.");
                let body = paired_body(&device.name, cx);
                frame(title, Some(lead), Some(body), None, done_footer(cx), cx)
            }
            State::Expired => frame(
                default_title,
                Some("This pairing link expired before a device connected.".into()),
                None,
                None,
                retry_footer(cx),
                cx,
            ),
            State::Failed(error) => frame(default_title, None, None, Some(error.clone()), retry_footer(cx), cx),
        }
    }
}

fn frame(
    title: String,
    lead: Option<String>,
    body: Option<AnyElement>,
    error: Option<String>,
    footer: AnyElement,
    cx: &App,
) -> impl IntoElement {
    let colors = look::colors(cx);
    let error = error.map(|msg| {
        let text = SharedString::from(msg);
        div()
            .id("share-hub-error")
            .role(Role::Alert)
            .aria_label(text.clone())
            .typeset(Type::Body)
            .text_color(colors.error)
            .child(text)
            .test_support()
    });
    v_flex()
        .id("share-hub-dialog")
        .w_full()
        .p(Space::XLarge)
        .gap(Space::Large)
        .typeset(Type::Body)
        .text_color(colors.text)
        .child(
            v_flex()
                .gap(Space::XSmall)
                .child(div().typeset(Type::Heading).child(title))
                .when_some(lead, |d, lead| d.child(div().text_color(colors.text_muted).child(lead))),
        )
        .children(body)
        .children(error)
        .child(footer)
        .test_support()
}

fn ready_row(link: &str, qr: Option<&QrMatrix>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let link_box = h_flex()
        .w_full()
        .px(Space::Medium)
        .py(Space::Small)
        .items_center()
        .bg(colors.inset)
        .border_1()
        .border_color(colors.edge)
        .corners(Corner::Control)
        .child(
            div()
                .id("pairing-link-text")
                .flex_1()
                .min_w_0()
                .truncate()
                .typeset(Type::Code)
                .text_color(colors.text)
                .child(link.to_string())
                .test_support(),
        );
    h_flex()
        .gap(Space::Large)
        .items_center()
        .when_some(qr, |d, matrix| d.child(qr::render(matrix, cx)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(Space::Small)
                .child(div().typeset(Type::Detail).emphasis().text_color(colors.text_muted).child("Pairing link"))
                .child(link_box)
                .child(
                    div()
                        .typeset(Type::Detail)
                        .text_color(colors.text_faint)
                        .child(format!("Works once · Grants control of this {MACHINE}’s workspaces.")),
                ),
        )
        .into_any_element()
}

fn paired_body(device_name: &str, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    v_flex()
        .gap(Space::Medium)
        .child(
            h_flex()
                .id("paired-device-card")
                .role(Role::Status)
                .aria_label(format!("{device_name}, Connected"))
                .w_full()
                .p(Space::Medium)
                .gap(Space::Small)
                .items_center()
                .bg(colors.inset)
                .border_1()
                .border_color(colors.edge)
                .corners(Corner::Control)
                .child(Icon::new(Lucide::CircleCheck).sized(IconSize::Large).text_color(colors.done))
                .child(
                    div().flex_1().min_w_0().truncate().typeset(Type::Body).emphasis().child(device_name.to_string()),
                )
                .child(div().typeset(Type::Detail).text_color(colors.done).child("Connected"))
                .test_support(),
        )
        .child(
            div()
                .typeset(Type::Detail)
                .text_color(colors.text_faint)
                .child("To remove access later, run roundhouse phone revoke in a terminal."),
        )
        .into_any_element()
}

fn waiting_line(words: &str, cx: &App) -> AnyElement {
    h_flex()
        .gap(Space::Small)
        .items_center()
        .text_color(look::colors(cx).text_muted)
        .child(look::task_ring(TaskRing::Running, cx))
        .child(words.to_string())
        .into_any_element()
}

fn ready_footer(copied: bool, cx: &mut Context<ShareHub>) -> AnyElement {
    h_flex()
        .justify_end()
        .gap(Space::Small)
        .child(close_button("cancel-share-hub", "Cancel", false, cx))
        .child(
            look::button(
                "copy-pairing-link",
                look::ButtonKind::Main,
                look::ControlSize::Large,
                if copied { "Copied" } else { "Copy link" },
            )
            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
        )
        .into_any_element()
}

fn retry_footer(cx: &mut Context<ShareHub>) -> AnyElement {
    h_flex()
        .justify_end()
        .gap(Space::Small)
        .child(close_button("close-share-hub", "Close", false, cx))
        .child(
            look::button("retry-pairing-link", look::ButtonKind::Main, look::ControlSize::Large, "Create new link")
                .on_click(cx.listener(|this, _, _, cx| this.generate(cx))),
        )
        .into_any_element()
}

fn close_only(label: &'static str, cx: &mut Context<ShareHub>) -> AnyElement {
    h_flex().justify_end().child(close_button("cancel-share-hub", label, false, cx)).into_any_element()
}

fn done_footer(cx: &mut Context<ShareHub>) -> AnyElement {
    h_flex().justify_end().child(close_button("done-share-hub", "Done", true, cx)).into_any_element()
}

fn close_button(id: &'static str, label: &'static str, primary: bool, cx: &mut Context<ShareHub>) -> look::Button {
    let kind = if primary { look::ButtonKind::Main } else { look::ButtonKind::Edged };
    look::button(id, kind, look::ControlSize::Large, label).on_click(cx.listener(|this, _, window, cx| {
        this.closed(cx);
        window.close_dialog(cx);
    }))
}
