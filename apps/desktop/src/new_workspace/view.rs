//! How the new-workspace dialog looks: the repository over a line the dialog's width, then what's attached and what's
//! typed, then the `+` menu, the model and effort, and Create, as the composer's bar has them, over plan mode's
//! texture while it's on.

use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestSupportExt as _, Window, div, px, transparent_black,
};

use look::Shaped as _;

use super::NewWorkspaceDialog;
use crate::attached;
use crate::composing::{self, Plan};
use crate::model_picker::model_picker;

impl Render for NewWorkspaceDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An edge of its own inside the dialog's, which shows only while files are dragged over it.
        let target = v_flex().id("new-workspace").w_full().border(look::EDGE).border_color(transparent_black());
        let target = composing::place(target, self.planning(), look::Corner::Card, cx, Self::toggle_plan_mode);
        let target = attached::takes_drops(target, cx, |_, paths, cx| Self::attach_paths(paths, cx))
            .child(self.header(cx))
            .child(
                v_flex()
                    .w_full()
                    .relative()
                    .children(
                        self.attached
                            .tiles("new-workspace-attachments", cx, Self::remove_attached)
                            .map(|t| t.px(look::Space::Small).pt(look::Space::Small)),
                    )
                    .child(self.prompt_field(cx))
                    .children(
                        (self.base_agent().is_some_and(protocol::model::AgentDescription::discovers_models)
                            && self.agent().is_some()
                            && !self.selection_available())
                        .then_some("The saved model or effort is unavailable. Choose again to create this workspace."),
                    )
                    .child(self.footer(cx))
                    .child(self.dictation.read(cx).edge()),
            );
        // The composer's surface and corners, since it's where a prompt is written, and its edge, which the line under
        // the repository is too. A plain dialog is only what's in it, so this is its fill and edge.
        let (background, edge) = self.colors(cx);
        div().w_full().bg(background).border(look::EDGE).border_color(edge).corners(look::Corner::Card).child(target)
    }
}

impl NewWorkspaceDialog {
    /// The repository it'll be made in, which opens a menu of the others.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let repos = &self.offer.repos;
        let name = repos.iter().find(|(id, _)| *id == self.repo).map_or_default(|(_, name)| name.clone());
        let (choices, current, this) = (repos.clone(), self.repo.clone(), cx.weak_entity());
        let menu = move |mut menu: look::Menu, _: &mut Window, _: &mut gpui_kit::App| {
            for (id, name) in &choices {
                let (this, id) = (this.clone(), id.clone());
                let checked = id == current;
                menu = menu.row(look::menu_row(name.clone()).chosen(checked).on_choose(move |_, cx| {
                    let _ = this.update(cx, |t, cx| t.set_repo(id.clone(), cx));
                }));
            }
            menu
        };
        h_flex()
            .w_full()
            .h(look::BarSize::Regular.height())
            .px(look::Space::Small)
            .border_b(look::EDGE)
            .border_color(self.colors(cx).1)
            .child(
                look::button("new-workspace-repo", look::ButtonKind::Borderless, look::ControlSize::Regular, name)
                    .menu(Anchor::TopLeft, menu),
            )
    }

    /// What's typed, which takes a paste of images or files as attachments.
    fn prompt_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let field = Textarea::new(&self.prompt)
            .on_paste(move |item, _, cx| this.update(cx, |_, cx| Self::paste(item, cx)).unwrap_or(false));
        composing::writing_area(&self.prompt, field, &self.dictation, cx)
            .id("new-workspace-writing")
            .test_support()
            // Room for the prompt's usual seven lines, shared with live words.
            .min_h(px(164.))
    }

    /// The `+` menu and the model and effort on the left, as in the composer's bar, and on the right Create, the
    /// dialog's one main action.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let bar = h_flex()
            .w_full()
            .px(look::Space::Medium)
            .pb(look::Space::Medium)
            .pt(look::Space::XSmall)
            .gap(look::Space::Small);
        if let Some(controls) = dictation_ui::Field::controls(&self.dictation, look::ControlSize::Regular, cx) {
            return bar.child(controls).into_any_element();
        }
        let this = cx.weak_entity();
        // Only once there's something to show, which a hub that doesn't describe its agents leaves to the hub.
        let agent = self.agent();
        let picker = self.shown().filter(|_| !self.models_waiting()).map(|(model, effort)| {
            model_picker("new-workspace-model", agent.as_ref(), &model, effort, None, move |picked, _, cx| {
                let _ = this.update(cx, |t, cx| t.pick(picked, cx));
            })
            .into_any_element()
        });
        let agents: Vec<_> = self.offer.agents.named().map(|a| (a.kind, a.name.clone())).collect();
        let agent_picker = (agents.len() > 1).then(|| {
            let this = cx.weak_entity();
            let current = self.base_agent().map(|a| a.kind);
            let name = self.base_agent().map_or("Agent", |a| a.name.as_str()).to_string();
            look::button("new-workspace-agent", look::ButtonKind::Borderless, look::ControlSize::Small, name).menu(
                Anchor::TopLeft,
                move |mut menu: look::Menu, _, _| {
                    for (kind, name) in &agents {
                        let (kind, this) = (*kind, this.clone());
                        menu = menu.row(look::menu_row(name.clone()).chosen(current == Some(kind)).on_choose(
                            move |_, cx| {
                                let _ = this.update(cx, |t, cx| t.pick_agent(kind, cx));
                            },
                        ));
                    }
                    menu
                },
            )
        });
        let field = self.prompt.focus_handle(cx);
        let plan = if self.can_plan() { Plan::of(self.planning()) } else { Plan::Unavailable };
        let plus =
            composing::plus_menu("new-workspace-add", plan, field, cx, |_, paths, cx| Self::attach_paths(paths, cx));
        let create = look::button("new-workspace-create", look::ButtonKind::Main, look::ControlSize::Regular, "Create")
            .after(look::key(look::KeyKind::Plain, "enter"))
            .off(!self.can_create())
            .on_click(cx.listener(|t, _, _, cx| t.create(cx)));
        bar.child(plus)
            // A row, so the picker keeps its own width and its caret stays beside what it says.
            .child(h_flex().flex_1().min_w_0().children(agent_picker).children(picker).children(self.models_status(cx)))
            .children(crate::dictating::microphone(&self.dictation, cx))
            .child(create)
            .into_any_element()
    }
}
