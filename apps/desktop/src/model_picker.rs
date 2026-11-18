//! The model and its effort, as one button that opens one menu with both, for a chat's composer and for a new
//! workspace's first prompt. What it offers is what the chat's agent is offered, as the hub describes it.

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div};
use protocol::capability::settings::Effort;
use protocol::model::{AgentDescription, ModelName};

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

/// What the picker's menu lists: the models with what each is called, then the levels.
#[derive(Debug, PartialEq, Eq)]
struct Offered {
    models: Vec<(ModelName, SharedString)>,
    efforts: Vec<protocol::capability::settings::EffortOffer>,
}

impl Offered {
    /// For a chat that runs `model` at `effort`: the models and levels its agent is offered, each level this build can
    /// name, or from a hub that doesn't describe its agent, only what the chat runs now, since nothing else is known to
    /// be one it takes.
    fn to(agent: Option<&AgentDescription>, model: &ModelName, effort: Effort) -> Offered {
        let Some(agent) = agent else {
            let models = vec![(model.clone(), SharedString::from(model.as_str().to_string()))];
            return Offered {
                models,
                efforts: effort
                    .as_str()
                    .is_some()
                    .then_some(effort)
                    .map(protocol::capability::settings::EffortOffer::Id)
                    .into_iter()
                    .collect(),
            };
        };
        let models = agent.models.iter().map(|choice| (choice.model.clone(), choice.label.clone().into())).collect();
        Offered { models, efforts: agent.effort_offers(model) }
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
        effort: effort.clone(),
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
        let effort = self
            .offered
            .efforts
            .iter()
            .find(|e| e.id() == &self.effort)
            .and_then(|e| e.label())
            .or_else(|| self.effort.label())
            .map(str::to_owned);
        let muted = look::colors(cx).text_muted;
        let (offered, model, chosen, switching, on_pick) =
            (self.offered, self.model, self.effort, self.switching, self.on_pick);
        look::button(self.id, look::ButtonKind::Borderless, look::ControlSize::Small, self.label)
            .when_some(effort, |button, effort| button.after(div().text_color(muted).child(effort)))
            .menu(Anchor::BottomLeft, move |menu, _, _| {
                menu_of(menu, &offered, &model, &chosen, switching.as_ref(), &on_pick)
            })
    }
}

/// The models, then the levels, with what's chosen checked. Each model but the chosen one says `switching` while it's
/// pointed at.
fn menu_of(
    mut menu: look::Menu,
    offered: &Offered,
    model: &ModelName,
    effort: &Effort,
    switching: Option<&SharedString>,
    on_pick: &OnPick,
) -> look::Menu {
    for (id, label) in offered.models.iter().cloned() {
        let row = look::menu_row(label).chosen(id == *model);
        let row = row.when_some(switching.filter(|_| id != *model).cloned(), look::MenuRow::tooltip);
        menu = menu.row(picks(row, Picked::Model(id), on_pick));
    }
    if offered.efforts.is_empty() {
        return menu;
    }
    menu = menu.separator().label("Effort");
    for level in &offered.efforts {
        let label = level.label().unwrap_or_default().to_owned();
        menu = menu.row(picks(
            look::menu_row(label).chosen(effort == level.id()),
            Picked::Effort(level.id().clone()),
            on_pick,
        ));
    }
    menu
}

fn picks(row: look::MenuRow, picked: Picked, on_pick: &OnPick) -> look::MenuRow {
    let on_pick = on_pick.clone();
    row.on_choose(move |window, cx| on_pick(picked.clone(), window, cx))
}

#[cfg(test)]
mod tests {
    use protocol::capability::settings::Settings;
    use protocol::model::{AgentKind, ModelChoice};

    use super::*;

    const OPUS: ModelName = ModelName::named("opus");

    /// An agent offered Opus and Fable, at two levels this build can name and one it can't.
    fn agent() -> AgentDescription {
        let offered = |model, label: &str| ModelChoice { model, label: label.to_string() };
        AgentDescription {
            capabilities: None,
            kind: AgentKind::Claude,
            name: "Claude Code".to_string(),
            usage_heading: "Claude usage".to_string(),
            models: vec![offered(OPUS, "Opus"), offered(ModelName::named("fable"), "Fable")],
            efforts: vec![Effort::High, Effort::Unknown, Effort::Max],
            defaults: Settings { model: Some(OPUS), effort: Some(Effort::High), ..Settings::default() },
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
        assert_eq!(
            offered.efforts.iter().map(protocol::capability::settings::EffortOffer::id).collect::<Vec<_>>(),
            [&Effort::High, &Effort::Max]
        );
    }

    #[test]
    fn with_no_description_the_menu_offers_only_what_the_chat_runs() {
        let offered = Offered::to(None, &OPUS, Effort::Xhigh);
        assert_eq!(
            offered,
            Offered {
                models: vec![(OPUS, "opus".into())],
                efforts: vec![protocol::capability::settings::EffortOffer::Id(Effort::Xhigh)]
            }
        );

        let newer = Offered::to(None, &OPUS, Effort::Unknown);
        assert!(newer.efforts.is_empty(), "a level from a newer hub can't be named, so it isn't offered");
    }

    #[test]
    fn every_level_has_a_name_and_one_from_a_newer_hub_has_none() {
        let names: Vec<_> = Effort::ALL.iter().map(Effort::label).collect();
        assert_eq!(names, [Some("Low"), Some("Medium"), Some("High"), Some("Extra high"), Some("Max"), Some("Ultra")]);
        assert_eq!(Effort::Unknown.label(), None);
    }
}
