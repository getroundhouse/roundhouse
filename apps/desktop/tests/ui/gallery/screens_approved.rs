#![cfg(test)]
use super::screens::SCREENS;
use super::{
    approval_chats, approved, background_chats, baselines, cache_screens, call_screens, capture, colours,
    compaction_chats, context_screens, controls, corners, diff_screens, failure_chats, home_screens, kinds_of_text,
    look_screens, mark_states, pull_requests, settings_screens, sidebar_screens, steps, subagent_chats, tool_screens,
    turn_screens, update_screens, workspace_screens,
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

    let picture = colours::draw(&text_system);

    report(&look_at(colours::NAME, colours::SPEC, &picture));
}

#[test]
fn every_kind_of_text_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = kinds_of_text::draw(&text_system);

    report(&look_at(kinds_of_text::NAME, kinds_of_text::SPEC, &picture));
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
            controls::NAME,
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
        .chain(compaction_chats::SCREENS)
        .chain(failure_chats::SCREENS)
        .chain(cache_screens::SCREENS)
        .chain(UPDATE_SCREENS)
        .chain(workspace_screens::SCREENS)
        .chain(turn_screens::SCREENS)
        .chain(call_screens::SCREENS)
        .chain(sidebar_screens::SCREENS)
        .chain(home_screens::SCREENS)
        .chain(settings_screens::SCREENS)
        .chain(look_screens::SCREENS)
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

    let picture = corners::draw(&text_system);

    report(&look_at(corners::NAME, corners::SPEC, &picture));
}

#[test]
fn every_control_size_looks_as_approved() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());

    let picture = controls::draw(&text_system);

    report(&look_at(controls::NAME, controls::SPEC, &picture));
}
