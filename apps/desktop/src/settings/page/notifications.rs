//! Notifications, the page that says how the app tells the person a chat needs them.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{Context, Window};

use super::{Control, Page, PageId, Setting};
use crate::settings::{change, get};
use crate::sounds;

pub(super) fn page() -> Page {
    let notify = Setting {
        name: "Notify when an agent needs you",
        does: "When it finishes, stops short or waits on you, unless it's in the chat you're looking at.",
        control: Control::Switch {
            is_on: Box::new(|cx| Some(get(cx).notify)),
            set: Rc::new(|on, cx| change(cx, |settings| settings.notify = on)),
        },
    };
    let play_sound = Setting {
        name: "Play a sound",
        does: "At the same moments, whether or not a notification shows.",
        control: Control::Switch {
            is_on: Box::new(|cx| Some(get(cx).play_sound)),
            set: Rc::new(|on, cx| change(cx, |settings| settings.play_sound = on)),
        },
    };
    let sound = Setting {
        name: "Sound",
        does: "Choosing one plays it.",
        control: Control::Choice {
            id: "settings-sound",
            chosen: Box::new(|cx| get(cx).sound.name().to_string().into()),
            menu: Rc::new(sounds_menu),
        },
    };
    Page {
        id: PageId::Notifications,
        name: "Notifications",
        icon: Lucide::Bell,
        settings: vec![notify, play_sound, sound],
    }
}

/// Chime, then a line, then the computer's own, with the one chosen ticked. Choosing one plays it, so it can be heard
/// before it's needed.
fn sounds_menu(mut menu: PopupMenu, _: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let chosen = get(cx).sound;
    for (at, sound) in sounds::choices(&chosen, cx).into_iter().enumerate() {
        if at == 1 {
            menu = menu.separator();
        }
        let item = PopupMenuItem::new(sound.name().to_string()).checked(sound == chosen);
        menu = menu.item(item.on_click(move |_, _, cx| {
            change(cx, |settings| settings.sound = sound.clone());
            sounds::play(&sound, cx);
        }));
    }
    menu
}
