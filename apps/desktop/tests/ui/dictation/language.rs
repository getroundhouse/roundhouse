//! Dictation language, chosen in Settings from a menu that scrolls, is the language what's dictated is heard in, and
//! is Auto until it's chosen.

use dictation::Language;
use gpui_kit::{ScrollDelta, TestAppContext, point, px};

use super::support::open_voice_settings;
use super::support::{ended, hold_option_space, let_go, ready, say};
use crate::support::{Stack, Ui};

/// Dictates into the composer, and waits for what's said to go in.
fn dictate(ui: &mut Ui, engine: &dictation::fake::FakeEngine) {
    hold_option_space(ui);
    say(ui, engine, "fix the test.");
    let_go(ui);
    ended(ui);
}

#[gpui_kit::test]
fn what_is_dictated_is_heard_in_the_language_chosen_in_settings(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    dictate(&mut ui, &engine);
    assert_eq!(engine.last_language(), Language::Auto, "until one is chosen");

    open_voice_settings(&mut ui);
    ui.click("settings-dictation-language");
    let ukrainian_shows = |ui: &mut Ui| ui.labelled("Ukrainian").iter().any(gpui_kit::test::ElementSnapshot::visible);
    assert!(!ukrainian_shows(&mut ui), "the last is below where the menu stops");
    ui.scroll("popup-menu", ScrollDelta::Pixels(point(px(0.), px(-2000.))));
    assert!(ukrainian_shows(&mut ui));
    ui.click_menu_item("Ukrainian");
    ui.press("escape");
    dictate(&mut ui, &engine);

    assert_eq!(engine.last_language(), Language::Ukrainian);
}
