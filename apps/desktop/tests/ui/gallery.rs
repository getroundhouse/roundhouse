//! Every screen of the app, drawn from made-up data, with no real hub and no window on screen. Each screen gets a
//! fake hub that answers from fixed data, in `gallery/screens.rs`, and the real app connects to it, so what's drawn
//! is exactly what the app draws. The pictures go in `.context/gallery/` at the repo's root, for looking at.

#![cfg(test)]

mod capture;
mod chats;
mod fake_hub;
mod screens;
mod world;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::PlatformTextSystem;

use screens::SCREENS;

/// A screen that never stops changing can't look the same from one run to the next, so it can't be compared.
#[test]
fn every_screen_draws_and_stops_changing() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/gallery");
    std::fs::create_dir_all(&out).unwrap();
    // Made once, since it's the slow part of starting, and every screen shapes text the same way. On its own,
    // because the platform it otherwise comes with has to be made on the main thread, and a test isn't on it.
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut unsettled = Vec::new();
    for screen in SCREENS {
        let picture = capture::take(screen, &text_system);
        picture.image.save(out.join(format!("{}.png", screen.name))).unwrap();
        if !picture.settled {
            unsettled.push(screen.name);
        }
    }
    assert!(unsettled.is_empty(), "these kept changing: {}", unsettled.join(", "));
}
