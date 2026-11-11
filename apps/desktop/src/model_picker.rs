//! The model and its effort, as one button that opens one menu with both, for a chat's composer and for a new
//! workspace's first prompt.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div};
use look::Hand as _;
use protocol::model::{Effort, ModelName};

/// Aliases, so these stay the newest of each without a release here. There's no `default`, since a chat always says which
/// model it runs.
pub(crate) const MODELS: [(ModelName, &str); 4] = [
    (ModelName::named("fable"), "Fable"),
    (ModelName::named("opus"), "Opus"),
    (ModelName::named("sonnet"), "Sonnet"),
    (ModelName::named("haiku"), "Haiku"),
];

/// What was picked in the model picker's menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Picked {
    Model(ModelName),
    Effort(Effort),
}

/// What the picker calls a model: its alias's name, or for any other, such as a full ID, the name itself.
pub(crate) fn model_label(model: &ModelName) -> &str {
    MODELS.iter().find(|(id, _)| id == model).map_or(model.as_str(), |(_, label)| label)
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

/// One button for the model and its effort, which opens one menu with both, showing what's chosen now.
pub(crate) fn model_picker(
    id: &'static str,
    model: &ModelName,
    effort: Effort,
    on_pick: impl Fn(Picked, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ModelPicker { id, model: model.clone(), effort, on_pick: Rc::new(on_pick) }
}

/// What's done with a pick, shared by every item in the menu.
type OnPick = Rc<dyn Fn(Picked, &mut Window, &mut App)>;

#[derive(IntoElement)]
struct ModelPicker {
    id: &'static str,
    model: ModelName,
    effort: Effort,
    on_pick: OnPick,
}

impl RenderOnce for ModelPicker {
    /// The model's name, then the effort in muted text, as `Opus  Extra high`. With a level this build can't name, only
    /// the model.
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let effort = effort_label(self.effort);
        let muted = cx.theme().muted_foreground;
        let label = model_label(&self.model).to_string();
        let (model, chosen, on_pick) = (self.model, self.effort, self.on_pick);
        Button::new(self.id)
            .hand()
            .ghost()
            .xsmall()
            .dropdown_caret(true)
            .label(label)
            .when_some(effort, |button, effort| button.child(div().text_color(muted).child(effort)))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| menu_of(menu, &model, chosen, &on_pick))
    }
}

/// The models, then the levels, with what's chosen checked.
fn menu_of(mut menu: PopupMenu, model: &ModelName, effort: Effort, on_pick: &OnPick) -> PopupMenu {
    for (id, label) in MODELS {
        menu = menu.item(picks(PopupMenuItem::new(label).checked(id == *model), Picked::Model(id), on_pick));
    }
    menu = menu.separator().label("Effort");
    for level in Effort::ALL {
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
    use super::*;

    #[test]
    fn a_model_with_no_name_here_still_shows() {
        assert_eq!(model_label(&ModelName::named("opus")), "Opus");
        assert_eq!(model_label(&ModelName::named("claude-something-new")), "claude-something-new");
    }

    #[test]
    fn the_default_is_one_of_the_models_offered() {
        assert!(MODELS.iter().any(|(id, _)| *id == protocol::model::DEFAULT_MODEL));
    }

    #[test]
    fn every_level_has_a_name_and_one_from_a_newer_hub_has_none() {
        let names: Vec<_> = Effort::ALL.into_iter().map(effort_label).collect();
        assert_eq!(names, [Some("Low"), Some("Medium"), Some("High"), Some("Extra high"), Some("Max")]);
        assert_eq!(effort_label(Effort::Unknown), None);
    }
}
