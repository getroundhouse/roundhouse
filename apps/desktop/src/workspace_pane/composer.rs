//! Where a prompt is written: what's attached, what's typed, and the bar under them, with the `+` menu and the model
//! picker at one end, and at the other what the agent says of itself, then Send or Stop. Shift-Tab turns plan mode on
//! and off, as the `+` menu does, and the composer shows it.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, Entity, Focusable as _, IntoElement, ParentElement, Pixels, Rems,
    Styled, Subscription, Window, div, px, rems,
};
use look::{Hand as _, InRegion as _, Off as _, Region};
use protocol::model::{Effort, ModelName, PermissionMode, SessionId, SessionSettingsChange};

use super::{PaneEvent, WorkspacePane};
use crate::composing::{self, Plan};
use crate::model_picker::{Picked, model_picker};
use crate::{StopAgent, attached, prompt_cache};

/// Its corners, which plan mode's texture stays inside.
const CORNER: Rems = rems(0.5);

/// Whether there's a hub to send what's typed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hub {
    Here,
    /// The app is connecting to one again, with the pane still on screen.
    Away,
}

/// The box a prompt is typed in, and what Enter does there: while the agent works, Enter steers it, and Cmd-Enter
/// queues what's typed for after it finishes. What's typed is kept as it changes.
pub(super) fn new_input(window: &mut Window, cx: &mut Context<WorkspacePane>) -> (Entity<TextareaState>, Subscription) {
    let input = cx.new(|cx| {
        TextareaState::new(window, cx)
            // Room for a paragraph before it grows, so a longer prompt doesn't start out cramped.
            .auto_grow(4, 12)
            .submit_on_enter(true)
            .placeholder("Ask the agent to do something")
    });
    let on_enter = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
        InputEvent::PressEnter { shift: false, secondary } => {
            if *secondary && this.session().is_some_and(|s| s.status.is_active()) {
                this.queue_typed(window, cx);
            } else {
                this.submit(window, cx);
            }
        }
        InputEvent::Change => this.keep_typed(false, cx),
        InputEvent::PressEnter { shift: true, .. } | InputEvent::Focus | InputEvent::Blur => {}
    });
    (input, on_enter)
}

impl WorkspacePane {
    /// For when the app starts connecting to a hub again, with this still on screen, so what's typed meanwhile isn't
    /// lost. Until [`Self::reconnect`], nothing typed is sent, and it stays in the composer: the hub may be one that's
    /// being stopped, which would take a prompt and then cut its agent off.
    pub fn hub_went_away(&mut self, cx: &mut Context<Self>) {
        self.hub = Hub::Away;
        cx.notify();
    }

    /// Sends what's typed, as Enter and Send do, unless the hub is away, when it stays typed for when it's back.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hub == Hub::Away {
            return;
        }
        let text = self.composer.read(cx).text().to_string();
        let Some(transcript) = self.transcript() else { return };
        // As `Compact now` does, so it's recorded and drawn as a compaction whichever way it was asked. To an agent that
        // doesn't compact when asked, or one the hub doesn't describe, it goes as a prompt, as any other text does.
        if self.attached.is_empty()
            && self.compacts()
            && let Some(rest) = after_compact(&text)
        {
            let keeping = (!rest.is_empty()).then(|| rest.to_string());
            transcript.update(cx, |t, cx| t.compact(keeping, Some(text.clone()), cx));
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
            self.keep_typed(true, cx);
            return cx.notify();
        }
        let files = self.attached.take();
        if transcript.update(cx, |t, cx| t.send_prompt_with(text, files, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
            // At once, so what was sent can't come back as a draft. A prompt that fails comes back as one.
            self.keep_typed(true, cx);
        }
        cx.notify();
    }

    /// Changes the showing chat's model or effort. The hub only agrees once a running agent has, so the picker changes
    /// when this comes back, and a refusal leaves it as it was.
    pub fn configure(&mut self, model: Option<ModelName>, effort: Option<Effort>, cx: &mut Context<Self>) {
        let Some(id) = self.active.clone() else { return };
        let change = SessionSettingsChange { model, permission_mode: None, effort };
        self.configure_chat(id, change, cx, |_, _, _| {});
    }

    /// Changes what `id` runs with, then calls `answered` with the mode the hub says the chat has now, or `None` if it
    /// refused.
    fn configure_chat(
        &mut self,
        id: SessionId,
        change: SessionSettingsChange,
        cx: &mut Context<Self>,
        answered: impl FnOnce(&mut Self, Option<PermissionMode>, &mut Context<Self>) + 'static,
    ) {
        let changing = self.client.start(move |client| client.session_configure(&id, change));
        cx.spawn(async move |this, cx| {
            let changed = changing.recv().await;
            let _ = this.update(cx, |t, cx| {
                let now = match changed {
                    Ok(session) => {
                        let mode = session.permission_mode;
                        t.took_reply(session, cx);
                        Some(mode)
                    }
                    Err(e) => {
                        cx.emit(PaneEvent::Error(format!("couldn't change that: {e}")));
                        None
                    }
                };
                answered(t, now, cx);
            });
        })
        .detach();
    }

    /// Whether the chat showing is planning. Its agent has full access otherwise.
    fn planning(&self) -> bool {
        self.session().is_some_and(|s| s.permission_mode == PermissionMode::Plan)
    }

    /// Plan mode as the `+` menu shows it. Only a chat has a mode, so there's none to change while the workspace's
    /// first is being made, or before the chat showing has loaded.
    fn plan(&self) -> Plan {
        match self.session() {
            Some(_) => Plan::of(self.planning()),
            None => Plan::Unavailable,
        }
    }

    /// Plan mode on or off for the chat showing, as Shift-Tab does. It changes once the agent has agreed. A press while
    /// the hub hasn't answered the one before goes from what that asked, so it undoes it.
    fn toggle_plan_mode(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session().map(|s| s.id.clone()) else { return };
        let planning = !self.plan_asked.get(&session).copied().unwrap_or_else(|| self.planning());
        self.want_plan_mode(session, planning, cx);
    }

    /// Asks for plan mode on or off in `session`, after any change already on its way there, which this then replaces.
    pub(super) fn want_plan_mode(&mut self, session: SessionId, planning: bool, cx: &mut Context<Self>) {
        if self.plan_asked.insert(session.clone(), planning).is_none() {
            self.ask_plan_mode(session, planning, cx);
        }
    }

    /// Asks for plan mode on or off in `session`, and once the hub has answered, for what the presses meanwhile asked,
    /// if that's different. One at a time, so the hub takes them in the order they were pressed.
    fn ask_plan_mode(&mut self, session: SessionId, planning: bool, cx: &mut Context<Self>) {
        let mode = if planning { PermissionMode::Plan } else { PermissionMode::FullAccess };
        let change = SessionSettingsChange { permission_mode: Some(mode), ..SessionSettingsChange::default() };
        self.configure_chat(session.clone(), change, cx, move |t, now, cx| {
            match (now, t.plan_asked.get(&session).copied()) {
                (Some(now), Some(wanted)) if (now == PermissionMode::Plan) != wanted => {
                    t.ask_plan_mode(session, wanted, cx);
                }
                _ => {
                    t.plan_asked.remove(&session);
                }
            }
        });
    }

    /// The model and its effort of the chat showing, and what its agent is offered.
    fn picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (now_ms, agent) = (look::now_ms(cx), self.agent());
        let model_and_effort =
            self.session().map(|s| (s.model.clone(), s.effort, prompt_cache::switching_note(s, now_ms)));
        let this = cx.weak_entity();
        let models = move |picked, _: &mut Window, cx: &mut gpui_kit::App| {
            let _ = this.update(cx, |t, cx| match picked {
                Picked::Model(model) => t.configure(Some(model), None, cx),
                Picked::Effort(effort) => t.configure(None, Some(effort), cx),
            });
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            // Only a chat has a model, so there's none to show while the workspace's first is being made.
            .when_some(model_and_effort, |row, (model, effort, switching)| {
                row.child(model_picker("model-picker", agent.as_ref(), &model, effort, switching, models))
            })
    }

    /// What's attached, what's typed, and below them the bar, over plan mode's texture while the agent is planning.
    /// Files dropped anywhere on it are attached, and so is a paste that holds images or files. It's centred under the
    /// transcript's column and a little wider, so the transcript's text sits inside its edges. While the agent waits on
    /// a question, the question is asked here instead, and what's typed and attached waits for it to go.
    pub(super) fn composer_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let queue = self.queue_card(cx);
        let around = |inside: AnyElement| transcript::around_column(v_flex().w_full().children(queue).child(inside));
        if let Some(question) = self.transcript().and_then(|t| t.read(cx).question().cloned()) {
            return around(question.into_any_element()).flex_none().pb_4().pt(transcript::ABOVE_THE_COMPOSER);
        }
        let planning = self.planning();
        let attached = self.attached_tiles(cx);
        let bar = self.bar(cx).into_any_element();
        let (surface, border) = composing::colors(planning, cx.theme().input, cx);
        let this = cx.weak_entity();
        let textarea = Textarea::new(&self.composer)
            .appearance(false)
            .on_paste(move |item, _, cx| this.update(cx, |t, cx| t.paste(item, cx)).unwrap_or(false));
        let composer =
            v_flex().region(Region::Composer).w_full().rounded(CORNER).border_1().border_color(border).bg(surface);
        let composer = composing::place(composer, planning, CORNER.into(), cx, Self::toggle_plan_mode);
        let composer = attached::takes_drops(composer, cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx))
            .children(attached)
            .child(div().w_full().px_2().pt_2().child(textarea))
            .child(bar);
        around(composer.into_any_element()).flex_none().pb_4().pt(transcript::ABOVE_THE_COMPOSER)
    }

    /// The row along the composer's bottom, which has two ends. The left one sets up the prompt: the `+` menu, then
    /// the model picker, which takes whatever room is left. The right one is the agent's: what it says of itself,
    /// each in a [`Slot`], and last the one main action, twice as far from them as they are from each other, so it
    /// stands apart.
    fn bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let field = self.composer.focus_handle(cx);
        let add =
            composing::plus_menu("add", self.plan(), field, cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx));
        let picker = self.picker(cx).into_any_element();
        let beside = self.beside_the_main_action(cx);
        // Left out when there's nothing in it, so it doesn't push the main action in by a gap.
        let beside = (!beside.is_empty()).then(|| h_flex().flex_none().gap_2().children(beside));
        let agents = h_flex().flex_none().gap_4().children(beside).child(self.main_action(cx));
        h_flex().w_full().px_3().pt_2().pb_3().gap_2().child(add).child(picker).child(agents)
    }

    /// What sits beside the main action, in the order it's drawn. Something new for the bar's right end goes here,
    /// and being a [`Slot`] is what lines it up and spaces it.
    fn beside_the_main_action(&self, cx: &mut Context<Self>) -> Vec<Slot> {
        let usage = self.usage(cx);
        self.usage_bars(usage.as_ref(), cx).into_iter().chain(self.context_ring(usage.as_ref(), cx)).collect()
    }

    /// Send, or Stop while the agent is in a turn. Work left in the background isn't one, since stopping a turn
    /// doesn't reach it.
    fn main_action(&self, cx: &mut Context<Self>) -> AnyElement {
        let working = self.session().is_some_and(|s| s.status.in_turn());
        if working {
            let stop_mark = cx.theme().button_primary_foreground;
            // The same square as Send, with a filled square rather than an icon, which would only be outlined.
            Button::new("stop")
                .hand()
                .primary()
                .small()
                .size_6()
                .p_0()
                .child(div().size_2p5().rounded_xs().bg(stop_mark))
                .accessibility_label("Stop")
                .tooltip_with_action("Stop", &StopAgent, None)
                .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                .into_any_element()
        } else {
            Button::new("send")
                .primary()
                .small()
                .icon(IconName::ArrowUp)
                // What's typed waits for the hub, as `hub_went_away` says.
                .off_while(self.hub == Hub::Away)
                .tooltip("Send")
                .on_click(cx.listener(|t, _, window, cx| t.submit(window, cx)))
                .into_any_element()
        }
    }
}

/// A place beside the main action: a square as big as it, with what it holds in its middle, or filling it if it's
/// made to. So things of different sizes line up with the main action, sit the same distance apart, and are as easy
/// to point at as each other. The bar takes only these, so nothing is put there any other way.
pub(super) struct Slot(Div);

impl Slot {
    /// How wide and tall a slot is, which is the main action's size: a small control's, in `docs/spec/look.md`.
    pub(super) const SIZE: Pixels = px(24.);

    pub(super) fn holding(holds: impl IntoElement) -> Self {
        Self(middle().flex_none().size(Self::SIZE).child(holds))
    }
}

impl IntoElement for Slot {
    type Element = Div;

    fn into_element(self) -> Div {
        self.0
    }
}

/// Something with whatever it's given in its middle.
pub(super) fn middle() -> Div {
    div().flex().items_center().justify_center()
}

/// What a typed `/compact` says after it, which is what to keep, and empty for a bare one. `None` for anything else.
fn after_compact(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix("/compact")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

#[cfg(test)]
mod tests {
    use super::after_compact;

    #[test]
    fn only_what_is_compact_asks_and_what_follows_it_is_what_to_keep() {
        assert_eq!(after_compact("/compact"), Some(""));
        assert_eq!(after_compact("  /compact  \n"), Some(""), "around it, it's still bare");
        assert_eq!(after_compact("/compact Keep the API design"), Some("Keep the API design"));
        assert_eq!(after_compact("/compact\nKeep the API design"), Some("Keep the API design"));
        assert_eq!(after_compact("/compactly"), None, "another word");
        assert_eq!(after_compact("Please /compact"), None, "said in passing");
    }
}
