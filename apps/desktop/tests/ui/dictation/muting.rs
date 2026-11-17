//! Mute sound while dictating, once it's chosen, mutes the sound output from when dictating starts, which for held
//! Space is once it dictates rather than once it listens, so a press let go of before then doesn't cut what's playing.

use gpui_kit::{AppContext as _, TestAppContext};

use super::support::{dictating, hold_option_space, let_go, press_space, ready};
use crate::settings::open_settings;
use crate::support::{Stack, Ui};
use desktop::workspace_pane::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};

/// Whether what's being dictated into the pane showing has the sound output muted.
fn muted(ui: &mut Ui) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, desktop::workspace_pane::WorkspacePane::mutes_output)
}

/// Chooses Mute sound while dictating in Settings, and goes back to the composer.
fn choose_muting(ui: &mut Ui) {
    open_settings(ui);
    ui.click_label("Mute sound while dictating");
    ui.press("escape");
}

#[gpui_kit::test]
fn dictating_mutes_the_sound_once_it_is_chosen(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    hold_option_space(&mut ui);
    assert!(dictating(&mut ui));
    assert!(!muted(&mut ui), "nothing is muted until it's chosen");
    let_go(&mut ui);

    choose_muting(&mut ui);
    hold_option_space(&mut ui);

    assert!(dictating(&mut ui));
    assert!(muted(&mut ui));
}

#[gpui_kit::test]
fn held_space_mutes_once_it_dictates_rather_than_once_it_listens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    choose_muting(&mut ui);

    press_space(&mut ui);
    ui.cx.executor().advance_clock(SPACE_LISTENS_AFTER);
    ui.frame();
    assert!(!muted(&mut ui), "listening unseen mutes nothing");
    ui.cx.executor().advance_clock(SPACE_DICTATES_AFTER.checked_sub(SPACE_LISTENS_AFTER).unwrap());
    ui.frame();

    assert!(dictating(&mut ui));
    assert!(muted(&mut ui));
}

#[gpui_kit::test]
fn quitting_while_dictating_lets_go_of_the_recording_so_the_sound_is_unmuted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    choose_muting(&mut ui);
    hold_option_space(&mut ui);
    assert!(muted(&mut ui));

    ui.cx.update(gpui_kit::App::shutdown);

    assert!(!dictating(&mut ui), "the recording, which unmutes as it's dropped, is kept past quitting");
}
