//! Each component's sheet, against its image in `docs/spec/screens`, and where it draws each of its kinds, for the
//! design system's page. The foundations' sheets and the app's screens are in `screens_approved.rs`.

#![cfg(test)]
use super::screens_approved::{look_at, report};
use super::{
    bar_kinds, button_states, cards, dialog_widths, field_states, key_kinds, lines, list_rows, menu_rows, small_things,
    switch_kinds, tab_widths, toggle_states, tooltips,
};
use gpui_kit::PlatformTextSystem;
use std::sync::Arc;

#[test]
fn every_kind_of_button_in_every_state_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = button_states::draw(&text_system);

    let mut problems = look_at(button_states::NAME, button_states::SPEC, &picture);
    problems.extend(places.look_at(button_states::NAME));
    report(&problems);
}

#[test]
fn every_kind_of_field_in_every_state_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = field_states::draw(&text_system);

    let mut problems = look_at(field_states::NAME, field_states::SPEC, &picture);
    problems.extend(places.look_at(field_states::NAME));
    report(&problems);
}

#[test]
fn every_state_of_a_toggle_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = toggle_states::draw(&text_system);

    report(&look_at(toggle_states::NAME, toggle_states::SPEC, &picture));
}

#[test]
fn every_kind_of_menu_row_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = menu_rows::draw(&text_system);

    let mut problems = look_at(menu_rows::NAME, menu_rows::SPEC, &picture);
    problems.extend(places.look_at(menu_rows::NAME));
    report(&problems);
}

#[test]
fn every_kind_of_switch_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = switch_kinds::draw(&text_system);

    let mut problems = look_at(switch_kinds::NAME, switch_kinds::SPEC, &picture);
    problems.extend(places.look_at(switch_kinds::NAME));
    report(&problems);
}

#[test]
fn every_width_of_dialog_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = dialog_widths::draw(&text_system);

    let mut problems = look_at(dialog_widths::NAME, dialog_widths::SPEC, &picture);
    problems.extend(places.look_at(dialog_widths::NAME));
    report(&problems);
}

#[test]
fn every_kind_and_width_of_card_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = cards::draw(&text_system);

    let mut problems = look_at(cards::NAME, cards::SPEC, &picture);
    problems.extend(places.look_at(cards::NAME));
    report(&problems);
}

#[test]
fn every_kind_of_bar_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = bar_kinds::draw(&text_system);

    let mut problems = look_at(bar_kinds::NAME, bar_kinds::SPEC, &picture);
    problems.extend(places.look_at(bar_kinds::NAME));
    report(&problems);
}

#[test]
fn every_width_of_tab_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = tab_widths::draw(&text_system);

    let mut problems = look_at(tab_widths::NAME, tab_widths::SPEC, &picture);
    problems.extend(places.look_at(tab_widths::NAME));
    report(&problems);
}

#[test]
fn every_size_of_list_row_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = list_rows::draw(&text_system);

    report(&look_at(list_rows::NAME, list_rows::SPEC, &picture));
}

#[test]
fn every_way_a_line_is_drawn_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = lines::draw(&text_system);

    report(&look_at(lines::NAME, lines::SPEC, &picture));
}

#[test]
fn every_tooltip_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = tooltips::draw(&text_system);

    report(&look_at(tooltips::NAME, tooltips::SPEC, &picture));
}

#[test]
fn every_kind_of_key_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = key_kinds::draw(&text_system);

    let mut problems = look_at(key_kinds::NAME, key_kinds::SPEC, &picture);
    problems.extend(places.look_at(key_kinds::NAME));
    report(&problems);
}

#[test]
fn every_pill_chip_badge_and_count_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();

    for (name, (picture, places)) in [
        (small_things::PILLS, small_things::pills(&text_system)),
        (small_things::CHIPS, small_things::chips(&text_system)),
        (small_things::BADGES, small_things::badges(&text_system)),
        (small_things::COUNTS, small_things::counts(&text_system)),
    ] {
        problems.extend(look_at(name, small_things::SPEC, &picture));
        problems.extend(places.look_at(name));
    }
    report(&problems);
}
