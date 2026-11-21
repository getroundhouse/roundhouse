//! A prompt's microphone, beside Send or Create, as the person's choice of dictation and how it stands have it: it dictates
//! once dictation is ready, and until then opens a popover, which offers to turn dictation on while it asks first, or
//! says how it stands once it's on, as `docs/spec/chats.md` says.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Entity, IntoElement as _, ParentElement as _, Styled as _, Subscription, Window, div,
};
use look::Region;

use crate::HoldToDictate;
use crate::dictating;
use crate::settings::Choice;
use crate::settings::{change, get};

/// The microphone, whichever it is, for tests to find it by, and its popover's buttons.
pub const MICROPHONE: &str = "dictate";
pub const TURN_ON: &str = "dictation-turn-on";
pub const HIDE: &str = "dictation-hide";

/// The popover's title, whether it offers dictation or says how it stands.
const TITLE: &str = "Dictation";
/// What the popover says while dictation asks first.
const OFFER: &str = "Hold Space, or click the microphone, and say what you want to write. Dictation needs a model of \
                     2.4 GB, downloaded once, which hears you on this Mac, so nothing you say leaves it.";

/// What the microphone does when it's pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Microphone {
    /// Starts dictating, or stops.
    Dictates,
    /// Offers to turn dictation on, while it asks first.
    Offers,
    /// Says how dictation stands, while it's on and not ready.
    Stands,
}

/// What the microphone is, or `None` where there's none: dictation is off, or this Mac can't run it.
fn state(cx: &App) -> Option<Microphone> {
    if !dictating::runs_here(cx) {
        return None;
    }
    match get(cx).dictation {
        Choice::Off => None,
        Choice::AskFirst => Some(Microphone::Offers),
        Choice::On if dictating::ready(cx) => Some(Microphone::Dictates),
        Choice::On => Some(Microphone::Stands),
    }
}

/// A microphone for any attached field. Question answers use the attachment without drawing this button.
pub(crate) fn microphone(field: &Entity<dictation_ui::Field>, cx: &mut App) -> Option<AnyElement> {
    let underway = field.read(cx).is_dictating();
    let colors = look::colors(cx);
    let (label, icon) = match if underway { Microphone::Dictates } else { state(cx)? } {
        Microphone::Dictates => {
            if !underway && !field.read(cx).can_dictate(cx) {
                return None;
            }
            let label = if underway { "Stop dictating" } else { "Dictate" };
            let field = field.downgrade();
            return Some(
                look::icon_button(MICROPHONE, look::ButtonKind::Borderless, look::ControlSize::Small, Lucide::Mic)
                    .on(underway)
                    .accessibility_label(label)
                    .tooltip_with_action(label, &HoldToDictate, None)
                    .on_click(move |_, window, cx| {
                        let _ = field.update(cx, |field, cx| field.toggle(window, cx));
                    })
                    .into_any_element(),
            );
        }
        Microphone::Offers => ("Dictate", Icon::new(Lucide::Mic)),
        Microphone::Stands => ("Dictation isn't ready", Icon::new(Lucide::Mic).text_color(colors.text_faint)),
    };
    let button = look::icon_button(MICROPHONE, look::ButtonKind::Borderless, look::ControlSize::Small, icon)
        .accessibility_label(label)
        .tooltip(label);
    Some(
        Popover::new("dictation-popover")
            .anchor(Anchor::BottomRight)
            .appearance(false)
            .trigger(button)
            .content(|_, window, cx| popover(window, cx))
            .into_any_element(),
    )
}

/// Kept while the popover is drawn, so its popover draws again as the model's download gets further, and only then.
struct FollowsTheDownload {
    _following: Subscription,
}

/// What the popover says: the offer to turn dictation on, while it asks first, and how it stands once it's on, with
/// what can be done about it.
fn popover(window: &mut Window, cx: &mut App) -> AnyElement {
    window.use_keyed_state("dictation-card-follows", cx, |_, cx| FollowsTheDownload {
        _following: dictating::observe_progress(cx, |_, _| true),
    });
    let colors = look::colors(cx);
    let card = look::card(look::CardKind::Over, look::CardWidth::Regular)
        .region(Region::DictationCard)
        .heading(look::card_heading(TITLE));
    if get(cx).dictation != Choice::AskFirst {
        return card.children(dictating::status_box(cx).map(|status| status.draw(cx))).into_any_element();
    }
    let turn_on = look::button(TURN_ON, look::ButtonKind::Main, look::ControlSize::Regular, "Turn on")
        .on_click(|_, _, cx| change(cx, |settings| settings.dictation = Choice::On));
    let hide = look::button(HIDE, look::ButtonKind::Outlined, look::ControlSize::Regular, "Hide the microphone");
    // The microphone goes, and its popover with it. The app returns focus to the visible prompt.
    let hide = hide.on_click(|_, _, cx| change(cx, |settings| settings.dictation = Choice::Off));
    card.under_heading(div().text_color(colors.text_muted).child(OFFER))
        .child(h_flex().gap(look::Space::Small).child(turn_on).child(hide))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::OFFER;

    #[test]
    fn what_it_says_it_downloads_is_what_there_is() {
        let size = format!("{} GB", crate::dictating::gigabytes(dictation::nemotron::MODEL.size_bytes()));
        assert!(OFFER.contains(&size), "{size}");
    }
}
