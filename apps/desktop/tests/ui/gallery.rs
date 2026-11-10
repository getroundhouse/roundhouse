//! Every screen of the app, drawn from made-up data, with no real hub and no window on screen, and checked against
//! its approved image in the spec. Each screen gets a fake hub that answers from fixed data, in
//! `gallery/screens.rs`, and the real app connects to it, so what's drawn is exactly what the app draws. What each
//! screen looks like now goes in `.context/gallery/` at the repo's root, for looking at. The app icon is drawn and
//! checked the same way, in `gallery/icon.rs`.

#![cfg(test)]

mod approved;
mod capture;
mod chats;
mod events;
mod fake_hub;
mod file_chats;
mod headless;
mod icon;
mod image_chats;
mod long_chats;
mod patch;
mod pictures;
mod pull_requests;
mod screens;
mod steps;
mod subagent_chats;
mod tool_chats;
mod world;
mod worlds;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::PlatformTextSystem;

use approved::Verdict;
use capture::Picture;
use screens::SCREENS;

/// Set to accept how every screen looks now, as its approved image.
const UPDATE: &str = "ROUNDHOUSE_UPDATE_SCREENS";

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
fn every_screen_looks_as_approved() {
    // Made once, since it's the slow part of starting, and every screen shapes text the same way. On its own,
    // because the platform it otherwise comes with has to be made on the main thread, and a test isn't on it.
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();
    for screen in SCREENS {
        problems.extend(look_at(screen.name, screen.spec, &capture::take(screen, &text_system)));
    }
    let names: Vec<&str> = SCREENS.iter().map(|s| s.name).chain([pull_requests::NAME]).collect();
    problems.extend(approved::without_a_screen(&names).into_iter().map(|name| format!("{name}.png has no screen")));
    report(&problems);
}

/// What's wrong with a picture: its spec doesn't show it, it keeps changing, or it isn't as approved. With
/// [`UPDATE`] set, it's accepted instead of compared. Either way it goes in `.context/gallery/` to look at.
fn look_at(name: &str, spec: &str, picture: &Picture) -> Vec<String> {
    let mut problems = Vec::new();
    if !approved::shown_in(name, spec) {
        problems.push(format!("docs/spec/{spec} doesn't show {name}"));
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/gallery");
    std::fs::create_dir_all(&out).unwrap();
    picture.image.save(out.join(format!("{name}.png"))).unwrap();
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

fn report(problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each screen looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it, \
         and changing a confirmed screen needs the owner's approval, as docs/spec/README.md says.",
        problems.join("\n")
    );
}
