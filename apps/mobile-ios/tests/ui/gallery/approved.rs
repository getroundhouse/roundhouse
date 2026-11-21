//! The gallery's tests: each screen and each sheet is drawn and checked against its approved image.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use spec_screens::{PHONE, report};

use super::capture;
use super::components::{self, SHEETS};
use super::screens::SCREENS;

/// The name of every picture the gallery takes, of a screen or of a component's sheet.
fn names() -> impl Iterator<Item = &'static str> {
    SCREENS.iter().map(|screen| screen.name).chain(SHEETS.iter().map(|sheet| sheet.name))
}

#[test]
fn every_phone_screen_looks_as_approved() {
    // Made once, since it's the slow part of starting, and every screen shapes text the same way.
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut problems = Vec::new();
    for screen in SCREENS {
        let picture = capture::take(screen, &text_system);
        problems.extend(PHONE.look_at(screen.name, screen.spec, &picture));
    }
    for sheet in SHEETS {
        let picture = components::take(sheet, &text_system);
        problems.extend(PHONE.look_at(sheet.name, sheet.spec, &picture));
    }
    let names: Vec<&str> = names().collect();
    problems.extend(PHONE.without_a_screen(&names).into_iter().map(|name| format!("phone/{name}.png has no screen")));
    report(&problems);
}

/// Each name is an image, so two screens with one would share it, and the second would only ever redraw the first.
#[test]
fn every_phone_screen_has_a_name_of_its_own() {
    let mut seen = std::collections::HashSet::new();
    let twice: Vec<String> = names()
        .filter(|name| !seen.insert(*name))
        .map(|name| format!("{name} is the name of more than one screen"))
        .collect();
    report(&twice);
}
