#![cfg(test)]
use super::screens::SCREENS;
use super::{
    approval_chats, approved, background_chats, baselines, button_states, cache_screens, call_screens, capture, cards,
    colours, compaction_chats, component_screens, context_screens, controls, corners, dialog_widths, dictation_screens,
    diff_screens, failure_chats, failures_in_a_row, field_states, heights, home_screens, icon_sizes, key_kinds,
    kinds_of_text, mark_states, menu_rows, own_turn_chats, pull_requests, settings_screens, sidebar_screens,
    small_things, spaces, steps, subagent_chats, switch_kinds, tab_widths, terminal_screens, toggle_states,
    tool_screens, tooltips, turn_screens, update_screens, workspace_screens,
};
use approved::Verdict;
use capture::Picture;
use diff_screens::DIFF_SCREENS;
use gpui_kit::PlatformTextSystem;
use std::path::PathBuf;
use std::sync::Arc;
use tool_screens::TOOL_SCREENS;
use update_screens::UPDATE_SCREENS;

/// Set to accept how every screen looks now, as its approved image.
pub(super) const UPDATE: &str = "ROUNDHOUSE_UPDATE_SCREENS";

#[test]
fn every_state_of_a_pull_request_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let missing = pull_requests::missing().into_iter().map(|kind| format!("no state of the kind {kind} is drawn"));
    let mut problems: Vec<String> = missing.collect();

    let picture = pull_requests::draw(&text_system);

    problems.extend(look_at(pull_requests::NAME, pull_requests::SPEC, &picture));
    report(&problems);
}

#[test]
fn every_state_of_the_mark_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = mark_states::draw(&text_system);

    report(&look_at(mark_states::NAME, mark_states::SPEC, &picture));
}

#[test]
fn every_colour_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = colours::draw(&text_system);

    let mut problems = look_at(colours::NAME, colours::SPEC, &picture);
    problems.extend(places.look_at(colours::NAME));
    report(&problems);
}

#[test]
fn every_kind_of_text_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = kinds_of_text::draw(&text_system);

    let mut problems = look_at(kinds_of_text::NAME, kinds_of_text::SPEC, &picture);
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
            problems.extend(look_at(screen.name, screen.spec, &picture));
        } else {
            // Cut to a frame that's wrong, so it's neither compared nor accepted, only kept to look at.
            keep_to_look_at(screen.name, &picture);
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
            dialog_widths::NAME,
            cards::NAME,
            menu_rows::NAME,
            field_states::NAME,
            heights::NAME,
            icon_sizes::NAME,
            spaces::NAME,
        ])
        .collect();
    problems.extend(approved::without_a_screen(&names).into_iter().map(|name| format!("{name}.png has no screen")));
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
        .chain(background_chats::SCREENS)
        .chain(own_turn_chats::SCREENS)
        .chain(compaction_chats::SCREENS)
        .chain(failure_chats::SCREENS)
        .chain(failures_in_a_row::SCREENS)
        .chain(cache_screens::SCREENS)
        .chain(terminal_screens::SCREENS)
        .chain(UPDATE_SCREENS)
        .chain(workspace_screens::SCREENS)
        .chain(turn_screens::SCREENS)
        .chain(call_screens::SCREENS)
        .chain(sidebar_screens::SCREENS)
        .chain(home_screens::SCREENS)
        .chain(settings_screens::SCREENS)
        .chain(super::git_screens::SCREENS)
        .chain(super::git_app_screens::SCREENS)
        .chain(dictation_screens::SCREENS)
        .chain(component_screens::SCREENS)
        .chain(super::prompt_screens::SCREENS)
}

/// What's wrong with a picture: its spec doesn't show it, it keeps changing, or it isn't as approved. With
/// [`UPDATE`] set, it's accepted instead of compared. Either way it goes in `.context/gallery/` to look at.
fn look_at(name: &str, spec: &str, picture: &Picture) -> Vec<String> {
    let mut problems = Vec::new();
    if !approved::shown_in(name, spec) {
        problems.push(format!("docs/spec/{spec} doesn't show {name}"));
    }
    keep_to_look_at(name, picture);
    // A picture that never stops changing can't look the same from one run to the next, so it can't be compared.
    if !picture.settled {
        problems.push(format!("{name} keeps changing"));
    } else if std::env::var_os(UPDATE).is_some() {
        approved::accept(name, &picture.image);
    } else {
        match approved::compare(name, &picture.image) {
            Verdict::Matches => {}
            Verdict::Differs => problems.push(format!("{name} no longer matches its image")),
            Verdict::Missing => problems.push(format!("{name} has no image")),
        }
    }
    problems
}

/// Puts the picture in `.context/gallery/`, to look at.
fn keep_to_look_at(name: &str, picture: &Picture) {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/gallery");
    std::fs::create_dir_all(&out).unwrap();
    picture.image.save(out.join(format!("{name}.png"))).unwrap();
}

fn report(problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each screen looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it, \
         and changing a confirmed screen needs the owner's approval, as docs/spec/README.md says.",
        problems.join("\n")
    );
}

#[test]
fn every_corner_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = corners::draw(&text_system);

    let mut problems = look_at(corners::NAME, corners::SPEC, &picture);
    problems.extend(places.look_at(corners::NAME));
    report(&problems);
}

#[test]
fn every_control_size_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = controls::draw(&text_system);

    let mut problems = look_at(controls::NAME, controls::SPEC, &picture);
    problems.extend(places.look_at(controls::NAME));
    report(&problems);
}

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
fn every_width_of_tab_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = tab_widths::draw(&text_system);

    let mut problems = look_at(tab_widths::NAME, tab_widths::SPEC, &picture);
    problems.extend(places.look_at(tab_widths::NAME));
    report(&problems);
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

#[test]
fn every_bar_and_row_height_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = heights::draw(&text_system);

    let mut problems = look_at(heights::NAME, heights::SPEC, &picture);
    problems.extend(places.look_at(heights::NAME));
    report(&problems);
}

#[test]
fn every_icon_size_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = icon_sizes::draw(&text_system);

    let mut problems = look_at(icon_sizes::NAME, icon_sizes::SPEC, &picture);
    problems.extend(places.look_at(icon_sizes::NAME));
    report(&problems);
}

#[test]
fn every_step_of_space_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let (picture, places) = spaces::draw(&text_system);

    let mut problems = look_at(spaces::NAME, spaces::SPEC, &picture);
    problems.extend(places.look_at(spaces::NAME));
    report(&problems);
}
