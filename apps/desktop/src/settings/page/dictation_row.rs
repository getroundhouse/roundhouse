//! The Dictation row of General: Ask first, On or Off, and under it a box that says how dictation stands, the language
//! what's dictated is heard in, and the switch that mutes the sound while dictating, as `docs/spec/settings.md` says.
//! Only where `dictation::HEARS`, and the switch only where `dictation::MUTES_OUTPUT`.

use std::rc::Rc;

use gpui_kit::{App, Window};

use super::{Control, Setting};
use crate::dictating::DictationChoice;
use crate::settings::{change, get};

/// The button that opens the choices.
const CHOICE: &str = "settings-dictation";

/// The button that opens the languages.
const LANGUAGE: &str = "settings-dictation-language";

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

pub(super) fn language() -> Setting {
    Setting {
        name: "Dictation language",
        does: "The language you dictate in. Auto tells which from what you say. Choosing yours hears it better, and \
               never writes it in another alphabet, as Auto can a short phrase said quickly, such as English in Cyrillic.",
        control: Control::Choice {
            id: LANGUAGE,
            chosen: Box::new(|cx| get(cx).dictation_language.name().into()),
            menu: Rc::new(languages),
        },
        status_box: None,
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
fn choices(mut menu: look::Menu, _: &mut Window, cx: &mut App) -> look::Menu {
    let chosen = get(cx).dictation;
    for choice in DictationChoice::ALL {
        let item = look::menu_row(choice.label()).chosen(choice == chosen);
        menu = menu.row(item.on_choose(move |_, cx| change(cx, |settings| settings.dictation = choice)));
    }
    menu
}

/// Auto, then every language by name, with the one chosen ticked, in a menu that scrolls, since they're too many to
/// fit.
fn languages(mut menu: look::Menu, _: &mut Window, cx: &mut App) -> look::Menu {
    let chosen = get(cx).dictation_language;
    for language in dictation::Language::ALL {
        let item = look::menu_row(language.name()).chosen(language == chosen);
        menu = menu.row(item.on_choose(move |_, cx| change(cx, |settings| settings.dictation_language = language)));
    }
    menu.scrolls()
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
