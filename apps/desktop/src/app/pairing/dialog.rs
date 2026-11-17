//! Connecting to another machine's hub from a short-lived pairing link: previews the target machine before connecting,
//! keeps the dialog open while reaching it, and reports failures inline.

use std::path::PathBuf;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window, div,
};
use look::{Corner, IconSize, IconSizing as _, Shaped as _, Space, TaskRing, Type, Typeset as _};
use protocol::pairing::PairingLink;

use super::words::{ParsedInput, connect_failure, parse_input};
use crate::app::{device_name, hubs, saved_hubs};

struct ConnectHub {
    data_dir: PathBuf,
    input: Entity<InputState>,
    device_name: Option<String>,
    connecting_to: Option<String>,
    error: Option<String>,
    shown: bool,
    _naming: Task<()>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) fn open(data_dir: PathBuf, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("roundhouse://pair?…"));
    let field = input.clone();
    let dialog = cx.new(|cx| ConnectHub::new(data_dir, field, cx));
    look::open_dialog(window, cx, move |_, cx| ConnectHub::dialog(&dialog, cx));
    input.update(cx, |input, cx| input.focus(window, cx));
}

impl ConnectHub {
    fn new(data_dir: PathBuf, input: Entity<InputState>, cx: &mut Context<Self>) -> Self {
        let reading = device_name::read_in_background(cx);
        let naming = cx.spawn(async move |this, cx| {
            let resolved = reading.await;
            let _ = this.update(cx, |this, cx| {
                this.device_name = Some(resolved);
                cx.notify();
            });
        });
        // Enter in the field confirms the dialog, which connects.
        let sub = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && this.error.take().is_some() {
                cx.notify();
            }
        });
        Self {
            data_dir,
            input,
            device_name: None,
            connecting_to: None,
            error: None,
            shown: true,
            _naming: naming,
            _subscriptions: vec![sub],
        }
    }

    fn closed(&mut self, _: &mut Context<Self>) {
        self.shown = false;
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connecting_to.is_some() {
            return;
        }
        let ParsedInput::Valid(parsed) = parse_input(&self.input.read(cx).value()) else {
            return;
        };
        let data = self.data_dir.clone();
        let intent = hubs::begin_selection(cx);
        let ready_name = self.device_name.clone();
        let lookup_name = ready_name.is_none().then(|| device_name::read_in_background(cx));
        let link = parsed.clone();
        let handle = window.window_handle();
        self.error = None;
        self.connecting_to = Some(parsed.hub_name.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let device = match (ready_name, lookup_name) {
                (Some(name), _) => name,
                (None, Some(reading)) => reading.await,
                (None, None) => unreachable!(),
            };
            let paired = cx.background_spawn(async move { client_core::pair(&link, &device) }).await;
            match paired {
                Ok(remote) => {
                    let key = saved_hubs::credential_key(&data, &remote.device_id);
                    if let Err(err) = saved_hubs::keep(&remote, key, cx).await {
                        let _ = this.update(cx, |this, cx| {
                            this.connecting_to = None;
                            this.error = Some(format!("Couldn’t save this hub’s credential: {err}"));
                            cx.notify();
                        });
                        return;
                    }
                    let still_shown = this.read_with(cx, |this, _| this.shown).unwrap_or(false);
                    if still_shown {
                        let _ = handle.update(cx, |_, window, cx| {
                            look::close_dialog(window, cx);
                            hubs::finish_selection(intent, remote, window, cx);
                        });
                    }
                }
                Err(err) => {
                    let (message, replace) = connect_failure(&parsed, &err);
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.connecting_to = None;
                            this.error = Some(message);
                            if replace && this.shown {
                                this.input.update(cx, |input, cx| {
                                    input.select_all(window, cx);
                                    input.focus(window, cx);
                                });
                            }
                            cx.notify();
                        });
                    });
                }
            }
        })
        .detach();
    }
}

impl ConnectHub {
    /// The dialog, as it stands now, built each time it's drawn, so it shows what's typed and how connecting goes.
    fn dialog(this: &Entity<Self>, cx: &App) -> look::Dialog {
        let connect = this.read(cx);
        let parsed = parse_input(&connect.input.read(cx).value());
        let connecting = connect.connecting_to.is_some();
        let preview = match &parsed {
            ParsedInput::Valid(link) => Some(preview_card(link, connect.device_name.as_deref(), cx)),
            ParsedInput::Empty | ParsedInput::Invalid(_) => None,
        };
        let can_submit = matches!(parsed, ParsedInput::Valid(_)) && !connecting;
        let feedback = connect.error.clone().or(match parsed {
            ParsedInput::Invalid(hint) => Some(hint),
            ParsedInput::Empty | ParsedInput::Valid(_) => None,
        });
        let label = if connecting {
            "Connecting…"
        } else if connect.error.is_some() && can_submit {
            "Try again"
        } else {
            "Connect"
        };
        let shown = v_flex()
            .id("connect-hub-dialog")
            .w_full()
            .gap(Space::Large)
            .child(
                look::field("pair-link-input", &connect.input, look::FieldKind::Boxed, look::ControlSize::Large)
                    .off(connecting),
            )
            .children(preview)
            .when_some(connect.connecting_to.as_deref(), |d, hub| d.child(reaching_line(hub, cx)))
            .when_some(feedback, |d, msg| d.child(feedback_alert(msg, cx)))
            .test_support();
        let (submitting, closing) = (this.clone(), this.clone());
        look::dialog("Connect another hub")
            .lead("Paste a single-use pairing link from the other machine’s hub switcher.")
            .child(shown)
            .closes(look::button("cancel-pair-hub", look::ButtonKind::Edged, look::ControlSize::Large, "Cancel"))
            .confirms(
                look::button("pair-hub", look::ButtonKind::Main, look::ControlSize::Large, label)
                    .off(!can_submit && !connecting)
                    .waiting(connecting),
            )
            // Connecting closes it once the hub is reached, so confirming only starts it.
            .on_confirm(move |window, cx| {
                submitting.update(cx, |this, cx| this.submit(window, cx));
                false
            })
            .on_close(move |_, cx| closing.update(cx, ConnectHub::closed))
    }
}

fn reaching_line(hub: &str, cx: &App) -> AnyElement {
    h_flex()
        .gap(Space::Small)
        .items_center()
        .text_color(look::colors(cx).text_muted)
        .child(look::task_ring(TaskRing::Running, cx))
        .child(format!("Reaching {hub}…"))
        .into_any_element()
}

fn feedback_alert(msg: String, cx: &App) -> AnyElement {
    let text = SharedString::from(msg);
    div()
        .id("connect-hub-error")
        .role(Role::Alert)
        .aria_label(text.clone())
        .typeset(Type::Body)
        .text_color(look::colors(cx).error)
        .child(text)
        .test_support()
        .into_any_element()
}

fn preview_card(link: &PairingLink, device_name: Option<&str>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let pairs_as = match device_name {
        Some(name) => format!("Pairs this computer as {name}"),
        None => "Pairs this computer".to_string(),
    };
    let a11y = format!("{}, {}, {pairs_as}", link.hub_name, link.hosts.join(" · "));
    let detail_indent = IconSize::Large.size() + Space::Small.size();
    let device_line =
        div().pl(detail_indent).truncate().typeset(Type::Detail).text_color(colors.text_faint).child(pairs_as);
    let device_line = match device_name {
        Some(_) => device_line.id("pair-link-device-name").test_support().into_any_element(),
        None => device_line.into_any_element(),
    };
    v_flex()
        .id("pair-link-preview")
        .role(Role::Group)
        .aria_label(a11y)
        .w_full()
        .p(Space::Medium)
        .gap(Space::XSmall)
        .bg(colors.inset)
        .border_1()
        .border_color(colors.edge)
        .corners(Corner::Control)
        .child(
            h_flex()
                .gap(Space::Small)
                .items_center()
                .child(Icon::new(Lucide::Server).sized(IconSize::Large).text_color(colors.text_muted))
                .child(div().flex_1().min_w_0().truncate().typeset(Type::Body).emphasis().child(link.hub_name.clone())),
        )
        .child(
            div()
                .pl(detail_indent)
                .truncate()
                .typeset(Type::Code)
                .text_color(colors.text_muted)
                .child(link.hosts.join(" · ")),
        )
        .child(device_line)
        .test_support()
        .into_any_element()
}
