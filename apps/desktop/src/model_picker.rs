//! The model and its effort, as one button that opens one menu with both, for a chat's composer and for a new
//! workspace's first prompt. What it offers is what the chat's agent is offered, as the hub describes it.

use std::rc::Rc;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use look::Hand as _;
use protocol::model::{AgentDescription, Effort, ModelName};

/// What was picked in the model picker's menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Picked {
    Model(ModelName),
    Effort(Effort),
}

/// What the picker calls a model: what its agent's description calls it, or for any other, such as a full ID, the name
/// itself.
pub(crate) fn model_label<'a>(model: &'a ModelName, agent: Option<&'a AgentDescription>) -> &'a str {
    agent.and_then(|agent| agent.label(model)).unwrap_or(model.as_str())
}

/// What the picker calls a level. `None` for one this build doesn't know, which it leaves unsaid.
pub(crate) fn effort_label(effort: Effort) -> Option<&'static str> {
    match effort {
        Effort::Low => Some("Low"),
        Effort::Medium => Some("Medium"),
        Effort::High => Some("High"),
        Effort::Xhigh => Some("Extra high"),
        Effort::Max => Some("Max"),
        Effort::Unknown => None,
    }
}

/// What the picker's menu lists: the models with what each is called, then the levels.
#[derive(Debug, PartialEq, Eq)]
struct Offered {
    models: Vec<(ModelName, SharedString)>,
    efforts: Vec<Effort>,
}

impl Offered {
    /// For a chat that runs `model` at `effort`: the models and levels its agent is offered, each level this build can
    /// name, or from a hub that doesn't describe its agent, only what the chat runs now, since nothing else is known to
    /// be one it takes.
    fn to(agent: Option<&AgentDescription>, model: &ModelName, effort: Effort) -> Offered {
        let Some(agent) = agent else {
            let models = vec![(model.clone(), SharedString::from(model.as_str().to_string()))];
            return Offered { models, efforts: effort.known().into_iter().collect() };
        };
        let models = agent.models.iter().map(|choice| (choice.model.clone(), choice.label.clone().into())).collect();
        Offered { models, efforts: agent.efforts.iter().filter_map(|effort| effort.known()).collect() }
    }
}

/// One button for the model and its effort, which opens one menu with both, showing what's chosen now and offering what
/// `agent` is. Pointing at another model says `switching`, when there's something to say of it, such as what the chat's
/// cache costs to write again.
pub(crate) fn model_picker(
    id: &'static str,
    agent: Option<&AgentDescription>,
    model: &ModelName,
    effort: Effort,
    switching: Option<String>,
    on_pick: impl Fn(Picked, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ModelPicker {
        id,
        label: model_label(model, agent).to_string(),
        model: model.clone(),
        effort,
        offered: Offered::to(agent, model, effort),
        switching: switching.map(Into::into),
        on_pick: Rc::new(on_pick),
    }
}

/// What's done with a pick, shared by every item in the menu.
type OnPick = Rc<dyn Fn(Picked, &mut Window, &mut App)>;

#[derive(IntoElement)]
struct ModelPicker {
    id: &'static str,
    /// What it calls the model chosen.
    label: String,
    model: ModelName,
    effort: Effort,
    offered: Offered,
    switching: Option<SharedString>,
    on_pick: OnPick,
}

impl RenderOnce for ModelPicker {
    /// The model's name, then the effort in muted text, as `Opus  Extra high`. With a level this build can't name, only
    /// the model.
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let effort = effort_label(self.effort);
        let muted = look::colors(cx).text_muted;
        let (offered, model, chosen, switching, on_pick) =
            (self.offered, self.model, self.effort, self.switching, self.on_pick);
        Button::new(self.id)
            .hand()
            .ghost()
            .xsmall()
            .dropdown_caret(true)
            .label(self.label)
            .when_some(effort, |button, effort| button.child(div().text_color(muted).child(effort)))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                menu_of(menu, &offered, &model, chosen, switching.as_ref(), &on_pick)
            })
    }
}

/// The models, then the levels, with what's chosen checked. Each model but the chosen one says `switching` while it's
/// pointed at.
fn menu_of(
    mut menu: PopupMenu,
    offered: &Offered,
    model: &ModelName,
    effort: Effort,
    switching: Option<&SharedString>,
    on_pick: &OnPick,
) -> PopupMenu {
    for (id, label) in offered.models.iter().cloned() {
        let item = match switching.filter(|_| id != *model).cloned() {
            // Named for assistive apps as a plain item is, which they're told the note of too, since they don't
            // read the tooltip out.
            Some(note) => PopupMenuItem::element(move |_, _| {
                let (note, described) = (note.clone(), note.clone());
                div()
                    .id(label.clone())
                    .flex_1()
                    .aria_label(label.clone())
                    .aria_description(described)
                    .tooltip(move |window, cx| Tooltip::new(note.clone()).build(window, cx))
                    .child(label.clone())
            }),
            None => PopupMenuItem::new(label),
        };
        menu = menu.item(picks(item.checked(id == *model), Picked::Model(id), on_pick));
    }
    menu = menu.separator().label("Effort");
    for &level in &offered.efforts {
        let label = effort_label(level).unwrap_or_default();
        menu = menu.item(picks(PopupMenuItem::new(label).checked(effort == level), Picked::Effort(level), on_pick));
    }
    menu
}

fn picks(item: PopupMenuItem, picked: Picked, on_pick: &OnPick) -> PopupMenuItem {
    let on_pick = on_pick.clone();
    item.on_click(move |_, window, cx| on_pick(picked.clone(), window, cx))
}

#[cfg(test)]
mod tests {
    use protocol::model::{AgentKind, ModelChoice, PermissionMode, SessionSettings};

    use super::*;

    const OPUS: ModelName = ModelName::named("opus");

    /// An agent offered Opus and Fable, at two levels this build can name and one it can't.
    fn agent() -> AgentDescription {
        let offered = |model, label: &str| ModelChoice { model, label: label.to_string() };
        AgentDescription {
            kind: AgentKind::Claude,
            name: "Claude Code".to_string(),
            usage_heading: "Claude usage".to_string(),
            models: vec![offered(OPUS, "Opus"), offered(ModelName::named("fable"), "Fable")],
            efforts: vec![Effort::High, Effort::Unknown, Effort::Max],
            modes: vec![PermissionMode::FullAccess],
            defaults: SessionSettings {
                model: OPUS,
                permission_mode: PermissionMode::FullAccess,
                effort: Effort::High,
            },
            features: Vec::new(),
            login: None,
        }
    }

    #[test]
    fn a_model_is_called_what_its_agent_calls_it_and_any_other_by_its_name() {
        let named = |id, agent: Option<&AgentDescription>| model_label(&ModelName::named(id), agent).to_string();

        assert_eq!(named("fable", Some(&agent())), "Fable");
        assert_eq!(named("claude-opus-5-5", Some(&agent())), "claude-opus-5-5", "a full ID the agent doesn't offer");
        assert_eq!(named("opus", None), "opus", "from a hub that doesn't describe its agent");
    }

    #[test]
    fn the_menu_offers_what_the_agent_is_offered_that_this_build_can_name() {
        let offered = Offered::to(Some(&agent()), &ModelName::named("claude-opus-5-5"), Effort::Low);

        let models: Vec<_> = offered.models.iter().map(|(model, label)| (model.as_str(), label.as_ref())).collect();
        assert_eq!(models, [("opus", "Opus"), ("fable", "Fable")]);
        assert_eq!(offered.efforts, [Effort::High, Effort::Max]);
    }

    #[test]
    fn with_no_description_the_menu_offers_only_what_the_chat_runs() {
        let offered = Offered::to(None, &OPUS, Effort::Xhigh);
        assert_eq!(offered, Offered { models: vec![(OPUS, "opus".into())], efforts: vec![Effort::Xhigh] });

        let newer = Offered::to(None, &OPUS, Effort::Unknown);
        assert!(newer.efforts.is_empty(), "a level from a newer hub can't be named, so it isn't offered");
    }

    #[test]
    fn every_level_has_a_name_and_one_from_a_newer_hub_has_none() {
        let names: Vec<_> = Effort::ALL.into_iter().map(effort_label).collect();
        assert_eq!(names, [Some("Low"), Some("Medium"), Some("High"), Some("Extra high"), Some("Max")]);
        assert_eq!(effort_label(Effort::Unknown), None);
    }
}
