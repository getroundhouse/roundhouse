//! What the chat runs with, under what's typed, as the desktop's composer has it: the model and its effort, from one
//! button that opens one menu with both, and plan mode, from the `+` menu, for an agent that can plan, said in plan
//! mode's copper while it's on. Each changes once the hub has agreed, and what's shown is what the chat's record says.

use client_core::refusals::couldnt;
use client_ui::{Picked, model_picker};
use gpui_kit::assets::IconName;
use gpui_kit::{
    Anchor, AnyElement, App, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use look::{ButtonKind, Type, Typeset as _};
use protocol::capability::settings::Settings;
use protocol::model::Session;

use super::view::ChatView;
use crate::chrome::TAPPED;

impl ChatView {
    /// Follows the list of models where the shown chat runs, for an agent that discovers its models, as the desktop's
    /// composer does. `again` asks even if it's been asked, as Retry does.
    pub(super) fn follow_models(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(session) = self.shown.as_ref().map(|shown| shown.session.clone()) else { return };
        let records = self.feed.snapshot();
        self.models.follow_chat(&self.hub, &records, &session, again, cx, |chat| &mut chat.models);
    }

    /// The `+` menu, the model picker, and plan mode while it's on, once there's a chat. While the agent is asked what
    /// it offers, its place says so, or what went wrong, with Retry.
    pub(super) fn settings_row(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(session) = self.shown.as_ref().map(|shown| &shown.session) else { return Vec::new() };
        let records = self.feed.snapshot();
        let mut row: Vec<AnyElement> = self.plus_menu(session, cx).into_iter().collect();
        let status = self.models.status(&records, cx, |chat, cx| chat.follow_models(true, cx));
        let picker = status.unwrap_or_else(|| {
            let agent = self.models.chat_agent(&records, session);
            let this = cx.weak_entity();
            let picked = move |picked: Picked, _: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |chat, cx| chat.pick(picked, cx));
            };
            let effort = session.effort.clone();
            model_picker("model-picker", agent.as_ref(), &session.model, effort, None, picked).into_any_element()
        });
        row.push(picker);
        if session.mode.plan {
            let (_, copper) = look::plan_colors(look::colors(cx).inset, cx);
            let said = div().id("planning").typeset(Type::Detail).text_color(copper).child("Plan mode");
            row.push(said.test_support().into_any_element());
        }
        row
    }

    /// The `+` menu, which holds plan mode, for a chat whose agent can plan.
    fn plus_menu(&self, session: &Session, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.feed.snapshot().agents().of(session.agent)?.plan_mode()?;
        let (this, planning) = (cx.weak_entity(), session.mode.plan);
        let button = look::icon_button("composer-menu", ButtonKind::Borderless, TAPPED, IconName::Plus)
            .accessibility_label("Plan mode")
            .menu(Anchor::BottomLeft, move |menu, _, _| {
                let this = this.clone();
                let row = look::menu_row("Plan mode").icon(IconName::Map).chosen(planning).on_choose(move |_, cx| {
                    let _ = this.update(cx, |chat, cx| chat.plan(!planning, cx));
                });
                menu.row(row)
            });
        Some(button.into_any_element())
    }

    /// Runs the shown chat on another model, at the effort its agent says goes with it, or at another effort.
    fn pick(&mut self, picked: Picked, cx: &mut Context<Self>) {
        let Some(session) = self.shown.as_ref().map(|shown| shown.session.clone()) else { return };
        let agent = self.models.chat_agent(&self.feed.snapshot(), &session);
        self.configure(&session, picked.change(agent.as_ref(), &session.effort), cx);
    }

    /// Plan mode on or off in the shown chat: the mode its agent plans in, or the one it starts a chat in.
    fn plan(&mut self, planning: bool, cx: &mut Context<Self>) {
        let Some(session) = self.shown.as_ref().map(|shown| shown.session.clone()) else { return };
        let records = self.feed.snapshot();
        let Some(agent) = records.agents().of(session.agent) else { return };
        if let Some(mode) = agent.mode_for(planning) {
            self.configure(&session, Settings { mode: Some(mode), ..Settings::default() }, cx);
        }
    }

    /// Asks the hub to change what `session` runs with, and says so if it refuses.
    fn configure(&self, session: &Session, change: Settings, cx: &mut Context<Self>) {
        let id = session.id.clone();
        let changing = self.client.start(move |client| client.session_configure(&id, change));
        cx.spawn(async move |this, cx| {
            if let Err(e) = changing.recv().await {
                let said = couldnt("change that", &e);
                let _ = this.update(cx, |chat, cx| chat.say(&said, cx));
            }
        })
        .detach();
    }
}
