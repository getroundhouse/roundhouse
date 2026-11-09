//! Every screen of the app, drawn from made-up data, with no real hub and no window on screen, and checked against
//! its approved image in the spec. Each screen gets a fake hub that answers from fixed data, in
//! `gallery/screens.rs`, and the real app connects to it, so what's drawn is exactly what the app draws. What each
//! screen looks like now goes in `.context/gallery/` at the repo's root, for looking at. The app icon is drawn and
//! checked the same way, in `gallery/icon.rs`.

#![cfg(test)]

mod approved;
mod capture;
mod chats;
mod fake_hub;
mod icon;
mod patch;
mod screens;
mod world;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::PlatformTextSystem;

use approved::Verdict;
use screens::SCREENS;

/// Set to accept how every screen looks now, as its approved image.
const UPDATE: &str = "ROUNDHOUSE_UPDATE_SCREENS";

#[test]
fn every_screen_looks_as_approved() {
    let update = std::env::var_os(UPDATE).is_some();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/gallery");
    std::fs::create_dir_all(&out).unwrap();
    // Made once, since it's the slow part of starting, and every screen shapes text the same way. On its own,
    // because the platform it otherwise comes with has to be made on the main thread, and a test isn't on it.
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();
    for screen in SCREENS {
        if !approved::shown_in(screen.name, screen.spec) {
            problems.push(format!("docs/spec/{} doesn't show {}", screen.spec, screen.name));
        }
        let picture = capture::take(screen, &text_system);
        picture.image.save(out.join(format!("{}.png", screen.name))).unwrap();
        // A screen that never stops changing can't look the same from one run to the next, so it can't be compared.
        if !picture.settled {
            problems.push(format!("{} keeps changing", screen.name));
        } else if update {
            approved::accept(screen.name, &picture.image);
        } else {
            match approved::compare(screen.name, &picture.image) {
                Verdict::Matches => {}
                Verdict::Differs => problems.push(format!("{} no longer matches its image", screen.name)),
                Verdict::Missing => problems.push(format!("{} has no image", screen.name)),
            }
        }
    }
    let names: Vec<&str> = SCREENS.iter().map(|s| s.name).collect();
    problems.extend(approved::without_a_screen(&names).into_iter().map(|name| format!("{name}.png has no screen")));
    assert!(
        problems.is_empty(),
        "{}\n\nWhat each screen looks like now is in .context/gallery/. If a change is meant, {UPDATE}=1 accepts it, \
         and changing a confirmed screen needs the owner's approval, as docs/spec/README.md says.",
        problems.join("\n")
    );
}
