#![cfg(test)]
use super::screens::SCREENS;
use super::{
    approval_chats, background_chats, banner_kinds, bar_kinds, baselines, button_states, cache_screens, call_screens,
    capture, cards, code_lines, colours, compaction_chats, component_screens, context_screens, controls,
    conversation_chats, corners, dialog_widths, dictation_screens, diff_screens, empty_kinds, failure_chats,
    failures_in_a_row, field_states, heights, home_screens, icon_sizes, key_kinds, kinds_of_text, lines, list_rows,
    mark_states, menu_rows, motions, own_turn_chats, proposal_chats, pull_requests, settings_screens, sidebar_screens,
    small_things, spaces, steps, strengths, subagent_chats, switch_kinds, tab_widths, terminal_screens, toggle_states,
    tool_screens, tooltips, turn_screens, update_screens, workspace_screens,
};
use diff_screens::DIFF_SCREENS;
use gpui_kit::PlatformTextSystem;
use spec_screens::{DESKTOP, report};
use std::sync::Arc;
use tool_screens::TOOL_SCREENS;
use update_screens::UPDATE_SCREENS;

#[test]
fn every_state_of_a_pull_request_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let missing = pull_requests::missing().into_iter().map(|kind| format!("no state of the kind {kind} is drawn"));
    let mut problems: Vec<String> = missing.collect();

    let picture = pull_requests::draw(&text_system);

    problems.extend(DESKTOP.look_at(pull_requests::NAME, pull_requests::SPEC, &picture));
    report(&problems);
}

#[test]
fn every_state_of_the_mark_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = mark_states::draw(&text_system);

    report(&DESKTOP.look_at(mark_states::NAME, mark_states::SPEC, &picture));
}

#[test]
fn every_colour_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = colours::draw(&text_system);

    let mut problems = DESKTOP.look_at(colours::NAME, colours::SPEC, &picture);
    problems.extend(places.look_at(colours::NAME));
    report(&problems);
}

#[test]
fn every_kind_of_text_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = kinds_of_text::draw(&text_system);

    let mut problems = DESKTOP.look_at(kinds_of_text::NAME, kinds_of_text::SPEC, &picture);
    problems.extend(places.look_at(kinds_of_text::NAME));
    report(&problems);
}

#[test]
fn text_of_every_kind_sits_on_body_texts_baseline_at_every_scale() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();

    for scale in baselines::SCALES {
        for (row, [body, small, code]) in baselines::bottoms(&text_system, scale).into_iter().enumerate() {
            if (small, code) != (body, body) {
                problems.push(format!("at {scale}x, row {row}: body ends on {body}, small on {small}, code on {code}"));
            }
        }
    }

    assert!(problems.is_empty(), "text off body text's baseline:\n{}", problems.join("\n"));
}

#[test]
fn every_screen_looks_as_approved() {
    // Made once, since it's the slow part of starting, and every screen shapes text the same way. On its own,
    // because the platform it otherwise comes with has to be made on the main thread, and a test isn't on it.
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();
    for screen in screens() {
        let (picture, framing) = capture::take(screen, &text_system);
        if framing.is_empty() {
            problems.extend(DESKTOP.look_at(screen.name, screen.spec, &picture));
        } else {
            // Cut to a frame that's wrong, so it's neither compared nor accepted, only kept to look at.
            DESKTOP.keep_to_look_at(screen.name, &picture);
            problems.extend(framing.into_iter().map(|problem| format!("{}: {problem}", screen.name)));
        }
    }
    let names: Vec<&str> = screens()
        .map(|s| s.name)
        .chain([
            pull_requests::NAME,
            mark_states::NAME,
            colours::NAME,
            kinds_of_text::NAME,
            corners::NAME,
            button_states::NAME,
            controls::NAME,
            key_kinds::NAME,
            toggle_states::NAME,
            tooltips::NAME,
            small_things::PILLS,
            small_things::CHIPS,
            small_things::BADGES,
            small_things::COUNTS,
            switch_kinds::NAME,
            tab_widths::NAME,
            bar_kinds::NAME,
            dialog_widths::NAME,
            cards::NAME,
            menu_rows::NAME,
            list_rows::NAME,
            lines::NAME,
            empty_kinds::NAME,
            banner_kinds::NAME,
            field_states::NAME,
            heights::NAME,
            code_lines::NAME,
            icon_sizes::NAME,
            spaces::NAME,
            strengths::NAME,
            motions::NAME,
        ])
        .collect();
    problems.extend(DESKTOP.without_a_screen(&names).into_iter().map(|name| format!("{name}.png has no screen")));
    report(&problems);
}

/// Each name is an image, so two screens with one would share it, and the second would only ever redraw the first.
/// Apart from drawing them, so it fails at once.
#[test]
fn every_screen_has_a_name_of_its_own() {
    let mut seen = std::collections::HashSet::new();
    let twice: Vec<String> = screens()
        .map(|screen| screen.name)
        .filter(|name| !seen.insert(*name))
        .map(|name| format!("{name} is the name of more than one screen"))
        .collect();
    report(&twice);
}

/// Every screen the gallery draws.
fn screens() -> impl Iterator<Item = &'static steps::Screen> {
    SCREENS
        .iter()
        .chain(super::codex_screens::SCREENS)
        .chain(context_screens::SCREENS)
        .chain(approval_chats::SCREENS)
        .chain(DIFF_SCREENS)
        .chain(TOOL_SCREENS)
        .chain(subagent_chats::SCREENS)
        .chain(proposal_chats::SCREENS)
        .chain(background_chats::SCREENS)
        .chain(own_turn_chats::SCREENS)
        .chain(compaction_chats::SCREENS)
        .chain(conversation_chats::SCREENS)
        .chain(failure_chats::SCREENS)
        .chain(failures_in_a_row::SCREENS)
        .chain(super::reference_screens::SCREENS)
        .chain(cache_screens::SCREENS)
        .chain(terminal_screens::SCREENS)
        .chain(UPDATE_SCREENS)
        .chain(workspace_screens::SCREENS)
        .chain(super::unseen_screens::SCREENS)
        .chain(turn_screens::SCREENS)
        .chain(call_screens::SCREENS)
        .chain(sidebar_screens::SCREENS)
        .chain(home_screens::SCREENS)
        .chain(settings_screens::SCREENS)
        .chain(super::git_screens::SCREENS)
        .chain(super::git_app_screens::SCREENS)
        .chain(super::provider_screens::SCREENS)
        .chain(super::tool_server_screens::SCREENS)
        .chain(dictation_screens::SCREENS)
        .chain(component_screens::SCREENS)
        .chain(super::prompt_screens::SCREENS)
}

#[test]
fn every_corner_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = corners::draw(&text_system);

    let mut problems = DESKTOP.look_at(corners::NAME, corners::SPEC, &picture);
    problems.extend(places.look_at(corners::NAME));
    report(&problems);
}

#[test]
fn every_motion_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = motions::draw(&text_system);

    let mut problems = DESKTOP.look_at(motions::NAME, motions::SPEC, &picture);
    problems.extend(places.look_at(motions::NAME));
    report(&problems);
}

#[test]
fn every_control_size_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = controls::draw(&text_system);

    let mut problems = DESKTOP.look_at(controls::NAME, controls::SPEC, &picture);
    problems.extend(places.look_at(controls::NAME));
    report(&problems);
}

#[test]
fn every_bar_and_row_height_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = heights::draw(&text_system);

    let mut problems = DESKTOP.look_at(heights::NAME, heights::SPEC, &picture);
    problems.extend(places.look_at(heights::NAME));
    report(&problems);
}

#[test]
fn every_spacing_of_codes_lines_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = code_lines::draw(&text_system);

    let mut problems = DESKTOP.look_at(code_lines::NAME, code_lines::SPEC, &picture);
    problems.extend(places.look_at(code_lines::NAME));
    report(&problems);
}

#[test]
fn every_icon_size_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = icon_sizes::draw(&text_system);

    let mut problems = DESKTOP.look_at(icon_sizes::NAME, icon_sizes::SPEC, &picture);
    problems.extend(places.look_at(icon_sizes::NAME));
    report(&problems);
}

#[test]
fn every_step_of_space_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = spaces::draw(&text_system);

    let mut problems = DESKTOP.look_at(spaces::NAME, spaces::SPEC, &picture);
    problems.extend(places.look_at(spaces::NAME));
    report(&problems);
}

#[test]
fn every_strength_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = strengths::draw(&text_system);

    let mut problems = DESKTOP.look_at(strengths::NAME, strengths::SPEC, &picture);
    problems.extend(places.look_at(strengths::NAME));
    report(&problems);
}
