//! The model and its effort, as one button that opens one menu with both, for a chat's composer and for a new
//! workspace's first prompt.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div};
use protocol::model::Effort;

/// Aliases, so these stay the newest of each without a release here. `default` is the account's own.
pub(crate) const MODELS: [(&str, &str); 5] =
    [("default", "Default model"), ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

/// What was picked in the model picker's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Picked {
    Model(&'static str),
    Effort(Effort),
}

pub(crate) fn model_label(model: Option<&str>) -> String {
    let model = model.unwrap_or("default");
    MODELS.iter().find(|(id, _)| *id == model).map_or(model, |(_, label)| label).to_string()
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

/// One button for the model and its effort, which opens one menu with both. `model` and `effort` are what's chosen
/// now, `None` for the user's own Claude settings.
pub(crate) fn model_picker(
    id: &'static str,
    model: Option<&str>,
    effort: Option<Effort>,
    on_pick: impl Fn(Picked, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ModelPicker { id, model: model.map(str::to_string), effort, on_pick: Rc::new(on_pick) }
}

/// What's done with a pick, shared by every item in the menu.
type OnPick = Rc<dyn Fn(Picked, &mut Window, &mut App)>;

#[derive(IntoElement)]
struct ModelPicker {
    id: &'static str,
    model: Option<String>,
    effort: Option<Effort>,
    on_pick: OnPick,
}

impl RenderOnce for ModelPicker {
    /// The model's name, then the effort in muted text, as `Opus  Extra high`. With no effort chosen, only the model.
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let effort = self.effort.and_then(effort_label);
        let muted = cx.theme().muted_foreground;
        let (model, chosen, on_pick) = (self.model.clone(), self.effort, self.on_pick);
        Button::new(self.id)
            .ghost()
            .xsmall()
            .dropdown_caret(true)
            .label(model_label(self.model.as_deref()))
            .when_some(effort, |button, effort| button.child(div().text_color(muted).child(effort)))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                menu_of(menu, model.as_deref(), chosen, &on_pick)
            })
    }
}

/// The models, then the levels, with what's chosen checked.
fn menu_of(mut menu: PopupMenu, model: Option<&str>, effort: Option<Effort>, on_pick: &OnPick) -> PopupMenu {
    let current = model.unwrap_or("default");
    for (id, label) in MODELS {
        menu = menu.item(picks(PopupMenuItem::new(label).checked(current == id), Picked::Model(id), on_pick));
    }
    menu = menu.separator().label("Effort");
    for level in Effort::ALL {
        let label = effort_label(level).unwrap_or_default();
        menu =
            menu.item(picks(PopupMenuItem::new(label).checked(effort == Some(level)), Picked::Effort(level), on_pick));
    }
    menu
}

fn picks(item: PopupMenuItem, picked: Picked, on_pick: &OnPick) -> PopupMenuItem {
    let on_pick = on_pick.clone();
    item.on_click(move |_, window, cx| on_pick(picked, window, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_with_no_name_here_still_shows() {
        assert_eq!(model_label(None), MODELS[0].1);
        assert_eq!(model_label(Some("opus")), "Opus");
        assert_eq!(model_label(Some("claude-something-new")), "claude-something-new");
    }

    #[test]
    fn every_level_has_a_name_and_one_from_a_newer_hub_has_none() {
        let names: Vec<_> = Effort::ALL.into_iter().map(effort_label).collect();
        assert_eq!(names, [Some("Low"), Some("Medium"), Some("High"), Some("Extra high"), Some("Max")]);
        assert_eq!(effort_label(Effort::Unknown), None);
    }
}
