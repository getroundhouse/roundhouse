//! Each weight the app asks for is drawn in a face of its own by the Mac's text system, since one that finds the same
//! face as another is a weight nobody sees. The app's emphasis asked for medium and was drawn as regular, unseen, until
//! `look` asked for what this text system finds Medium at.

#![cfg(test)]

use gpui_kit::{FontId, FontWeight, PlatformTextSystem as _, Styled as _, div, font};
use look::Typeset as _;

/// The face the system font is drawn in at `weight`.
fn face(weight: FontWeight) -> FontId {
    let text_system = gpui_apple::AppleTextSystem::new();
    let mut system = font(".SystemUIFont");
    system.weight = weight;
    text_system.font_id(&system).unwrap_or_else(|error| panic!("the system font at {weight:?}: {error}"))
}

#[test]
fn emphasis_and_what_is_marked_strong_are_each_drawn_in_a_face_of_their_own() {
    let emphasis = div().emphasis().style().text.font_weight.unwrap_or_default();

    assert_ne!(face(emphasis), face(FontWeight::NORMAL), "emphasis is drawn in the regular face");
    assert_ne!(face(emphasis), face(look::STRONG), "emphasis is drawn as what's marked strong is");
    assert_ne!(face(look::STRONG), face(FontWeight::BOLD), "what's marked strong is drawn bold");
}
