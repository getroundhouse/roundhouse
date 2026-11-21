//! The Speech row of Voice: Ask first, On or Off, and under it a box that says how the voice stands, then how fast
//! it reads, as `docs/spec/settings.md` says. Only where `sound::HEARS`, since conversation mode, which reads with it,
//! needs dictation too.

use std::rc::Rc;

use conversation::Speed;
use gpui_kit::{App, Window};

use super::{Control, Setting};
use crate::settings::{change, get};

/// The button that opens the choices.
const CHOICE: &str = "settings-speech";

/// The button that opens the speeds.
const SPEED: &str = "settings-speech-speed";

pub(super) fn setting() -> Setting {
    Setting {
        name: "Speech",
        does: "Hear the agent's replies read out in conversation mode. It needs a voice of 65 MB, downloaded once, \
               which speaks on this Mac. Ask first lets conversation mode offer to turn it on.",
        control: Control::ask_first(CHOICE, |settings| settings.speech, |settings, choice| settings.speech = choice),
        status_box: Some(Box::new(crate::conversing::status_box)),
    }
}

pub(super) fn speed() -> Setting {
    Setting {
        name: "Speech speed",
        does: "How fast replies are read out. Faster is about 1.2 times as fast as Normal, and Fastest about 1.4 times, \
               the fastest this voice speaks well.",
        control: Control::Choice {
            id: SPEED,
            chosen: Box::new(|cx| get(cx).speech_speed.name().into()),
            menu: Rc::new(speeds),
        },
        status_box: None,
    }
}

/// Every speed, slowest first, with the one chosen ticked.
fn speeds(mut menu: look::Menu, _: &mut Window, cx: &mut App) -> look::Menu {
    let chosen = get(cx).speech_speed;
    for speed in Speed::ALL {
        let item = look::menu_row(speed.name()).chosen(speed == chosen);
        menu = menu.row(item.on_choose(move |_, cx| change(cx, |settings| settings.speech_speed = speed)));
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::setting;

    #[test]
    fn what_it_says_it_downloads_is_what_there_is() {
        let size = crate::conversing::size();
        assert!(setting().does.contains(&size), "{size}");
    }
}
