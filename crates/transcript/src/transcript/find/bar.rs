//! The bar `Cmd-F` opens at the top right of the chat: what's looked for, how many matches there are and which is
//! shown, and buttons to go up and down them and to close it.

use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _,
    Subscription, Task, TestSupportExt as _, Window, div, px,
};
use look::{Hand as _, InRegion as _, Off as _, Region, Typeset as _};

use crate::transcript::Transcript;
use client_core::session_log::SessionLog;

/// How long typing pauses before what's typed is looked for, so a word isn't looked for a letter at a time.
const PAUSE: Duration = Duration::from_millis(150);
/// Wide enough for a few words and "999 of 1000".
const WIDTH: Pixels = px(340.);
/// In from the pane's right, clear of the outline's ticks, and down from the tabs as they are.
const RIGHT: Pixels = px(32.);
const TOP: Pixels = px(12.);
const HEIGHT: Pixels = px(36.);
/// How far down the chat the bar reaches, `TOP` and `HEIGHT`, which hides what's above it.
pub(super) const BOTTOM: Pixels = px(48.);

pub(crate) struct Bar {
    input: Entity<InputState>,
    /// Waiting for typing to pause.
    waiting: Option<Task<()>>,
    _typing: Subscription,
}

impl Bar {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Transcript>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in chat"));
        let typing = cx.subscribe_in(&input, window, |t, _, event: &InputEvent, window, cx| match event {
            InputEvent::Change => t.typed(cx),
            InputEvent::PressEnter { shift, .. } => t.find_next(!shift, window, cx),
            InputEvent::Focus | InputEvent::Blur => {}
        });
        Self { input, waiting: None, _typing: typing }
    }

    /// Puts focus in the bar, with what's in it selected, so typing replaces it.
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Transcript>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// What's typed in it.
    pub(crate) fn query(&self, cx: &gpui_kit::App) -> String {
        self.input.read(cx).value().to_string()
    }
}

impl Transcript {
    /// Looks for what's typed once typing pauses.
    fn typed(&mut self, cx: &mut Context<Self>) {
        let Some(finding) = &mut self.finding else { return };
        finding.bar.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE).await;
            // Unless Enter has looked for it already.
            let _ = this.update(cx, |t, cx| {
                if t.typing(cx) {
                    t.find_again(cx);
                }
            });
        }));
    }

    /// Whether what's typed hasn't been looked for yet, because typing hasn't paused.
    pub(crate) fn typing(&self, cx: &gpui_kit::App) -> bool {
        self.finding.as_ref().is_some_and(|finding| {
            finding.query.as_ref().map_or("", |query| query.needle.query()) != finding.bar.query(cx)
        })
    }

    /// The bar, over the top right of the chat, if it's open.
    pub(crate) fn find_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let finding = self.finding.as_ref()?;
        let colors = look::colors(cx);
        let muted = colors.text_muted;
        let everything_held = self.session.read(SessionLog::holds_every_event);
        let says = finding.query.as_ref().map(|query| query.says(everything_held)).unwrap_or_default();
        let none = finding.query.as_ref().is_none_or(|query| query.counts.total() == 0);
        let step = |id: &'static str, icon: IconName, what: &'static str, forward: bool| {
            Button::new(id)
                .off_while(none)
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(what)
                .on_click(cx.listener(move |t, _, window, cx| t.find_next(forward, window, cx)))
        };
        let up = step("find-previous", IconName::ChevronUp, "Previous match", false);
        let down = step("find-next", IconName::ChevronDown, "Next match", true);
        let close = Button::new("find-close")
            .hand()
            .ghost()
            .xsmall()
            .icon(IconName::Close)
            .tooltip("Close")
            .on_click(cx.listener(|t, _, _, cx| t.close_find(cx)));
        let bar = h_flex()
            .w(WIDTH)
            .h(HEIGHT)
            .pl_2()
            .pr_1()
            .gap_1()
            .child(
                Input::new(&finding.bar.input)
                    .appearance(false)
                    .small()
                    .prefix(Icon::new(IconName::Search).text_color(muted))
                    .flex_1(),
            )
            .child(
                div()
                    .id("find-says")
                    .flex_none()
                    .typeset(look::Type::Detail)
                    .text_color(muted)
                    .child(says)
                    .test_support(),
            )
            .child(up)
            .child(down)
            .child(close);
        let raised = look::raised(cx)
            .region(Region::FindBar)
            .on_action(cx.listener(|t, _: &Escape, _, cx| t.close_find(cx)))
            .child(bar);
        // A press on it is its own, not the chat's under it, but the wheel still scrolls the chat.
        Some(div().absolute().top(TOP).right(RIGHT).block_mouse_except_scroll().child(raised))
    }
}
