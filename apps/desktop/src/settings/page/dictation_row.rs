//! The Dictation row of General: Ask first, On or Off, and under it a box that says how dictation stands, and the
//! switch under it that mutes the sound while dictating, as `docs/spec/settings.md` says. Only where `dictation::HEARS`,
//! and the switch only where `dictation::MUTES_OUTPUT`.

use std::rc::Rc;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{Context, Window};

use super::{Control, Setting};
use crate::dictating::DictationChoice;
use crate::settings::{change, get};

/// The button that opens the choices.
const CHOICE: &str = "settings-dictation";

pub(super) fn setting() -> Setting {
    Setting {
        name: "Dictation",
        does: "Dictate prompts and written answers by holding Space in their text fields. Prompts also have a microphone button. \
               It needs a model of 2.4 GB, downloaded once, which hears you on this Mac, so nothing you say leaves \
               it. Ask first shows the microphone, and downloads the model once you click it and say so.",
        control: Control::Choice {
            id: CHOICE,
            chosen: Box::new(|cx| get(cx).dictation.label().into()),
            menu: Rc::new(choices),
        },
        status_box: Some(Box::new(crate::dictating::status_box)),
    }
}

pub(super) fn muting() -> Setting {
    Setting {
        name: "Mute sound while dictating",
        does: "Mute this Mac's sound while you dictate, and unmute it once you stop, so music or a video doesn't drown \
               you out. A display's speakers, and some USB speakers and amps, whose volume only they can set, play on.",
        control: Control::Switch {
            is_on: Box::new(|cx| Some(get(cx).mute_while_dictating)),
            set: Rc::new(|on, cx| change(cx, |settings| settings.mute_while_dictating = on)),
        },
        status_box: None,
    }
}

/// Ask first, On and Off, with the one chosen ticked.
fn choices(mut menu: PopupMenu, _: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let chosen = get(cx).dictation;
    for choice in DictationChoice::ALL {
        let item = PopupMenuItem::new(choice.label()).checked(choice == chosen);
        menu = menu.item(item.on_click(move |_, _, cx| change(cx, |settings| settings.dictation = choice)));
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::setting;

    #[test]
    fn what_it_says_it_downloads_is_what_there_is() {
        let size = format!("{} GB", crate::dictating::gigabytes(dictation::nemotron::MODEL.size_bytes()));
        assert!(setting().does.contains(&size), "{size}");
    }
}
