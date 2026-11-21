//! Mute sound and Pause media while dictating, once each is chosen, quiet what's playing from when dictating starts,
//! which for held Space is once it dictates rather than once it listens, so a press let go of before then doesn't cut
//! what's playing.

use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use sound::Quieting;

use super::support::open_voice_settings;
use super::support::{dictating, hold_option_space, let_go, press_space, ready};
use crate::support::{Stack, Ui};
use desktop::workspace_pane::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};

/// Whether what's being dictated quiets what's playing `how` says.
fn quiets(ui: &mut Ui, how: Quieting) -> bool {
    ui.cx.read(|cx| dictation_ui::quiets_while_dictating(how, cx))
}

/// Which of `Quieting::ALL` what's being dictated quiets what's playing in.
fn quieted(ui: &mut Ui) -> Vec<Quieting> {
    Quieting::ALL.into_iter().filter(|how| quiets(ui, *how)).collect()
}

/// Chooses the switch for `how` in Settings, at the bottom of Voice, and goes back to the composer.
fn choose(ui: &mut Ui, how: Quieting) {
    open_voice_settings(ui);
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    ui.click_label(match how {
        Quieting::MuteOutput => "Mute sound while dictating",
        Quieting::PauseMedia => "Pause media while dictating",
    });
    ui.press("escape");
}

/// Chooses every switch.
fn choose_all(ui: &mut Ui) {
    for how in Quieting::ALL {
        choose(ui, how);
    }
}

#[gpui_kit::test]
fn dictating_quiets_each_way_once_it_is_chosen(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    hold_option_space(&mut ui);
    assert!(dictating(&mut ui));
    assert_eq!(quieted(&mut ui), [], "nothing is quieted until it's chosen");
    let_go(&mut ui);

    let mut chosen = Vec::new();
    for how in Quieting::ALL {
        choose(&mut ui, how);
        chosen.push(how);
        hold_option_space(&mut ui);

        assert!(dictating(&mut ui));
        assert_eq!(quieted(&mut ui), chosen);
        let_go(&mut ui);
    }
}

#[gpui_kit::test]
fn held_space_quiets_once_it_dictates_rather_than_once_it_listens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    choose_all(&mut ui);

    press_space(&mut ui);
    ui.cx.executor().advance_clock(SPACE_LISTENS_AFTER);
    ui.frame();
    assert_eq!(quieted(&mut ui), [], "listening unseen quiets nothing");
    ui.cx.executor().advance_clock(SPACE_DICTATES_AFTER.checked_sub(SPACE_LISTENS_AFTER).unwrap());
    ui.frame();

    assert!(dictating(&mut ui));
    assert_eq!(quieted(&mut ui), Quieting::ALL);
}

#[gpui_kit::test]
fn quitting_while_dictating_lets_go_of_the_recording_so_what_it_quieted_is_put_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    choose_all(&mut ui);
    hold_option_space(&mut ui);
    assert_eq!(quieted(&mut ui), Quieting::ALL);

    ui.cx.update(gpui_kit::App::shutdown);

    assert!(
        !dictating(&mut ui),
        "the recording, which puts back what it quieted as it's dropped, is kept past quitting"
    );
}
