//! The comparisons that draw a changed image in one place: one side at a time, flipped between, so what changed jumps
//! out as it flips, or as what changed, with an outline around each place. Both sides take the place of the bigger,
//! from its top left, so a pixel stays where it is as they flip.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, div, pattern_slash, px,
};
use look::{Hand as _, pixels};

use super::{ImageDiff, Outcome, Shown, changed_colour};
use crate::file_image::{FileImage, Placed, edged, fitted, on_checkerboard};
use crate::file_view::placeholder;

/// How far the outline around a place that changed stands off it on the screen, so it doesn't hide the change.
const STAND_OFF: f32 = 4.;
/// How near two outlines can come on the screen before their places are drawn as one.
const NEARLY: f32 = 2.;
/// How strong the outline is, and the wash inside it, of the colour of what changed.
const OUTLINE: f32 = 0.55;
const WASH: f32 = 0.08;
/// How strong the stripes across what only one image covers are, of the muted colour.
const STRIPES: f32 = 0.35;

/// The side shown, which a click flips, as Space does. Before and After in the bar choose one.
pub(super) fn flip(before: &FileImage, after: &FileImage, shown: Shown, view: WeakEntity<ImageDiff>) -> AnyElement {
    let covers = (before.width.max(after.width), before.height.max(after.height));
    let image = match shown {
        Shown::Before => before.clone(),
        Shown::After => after.clone(),
    };
    let picture = fitted(covers, Placed::Top, move |scale, cx| image.within(covers, scale, cx));
    let flipping = div().id("images-flip").flex_1().w_full().min_h_0().hand().on_click(move |_, _, cx| {
        let _ = view.update(cx, ImageDiff::flip);
    });
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .p_6()
        .child(flipping.child(picture.size_full()).test_support())
        .into_any_element()
}

/// What changed, once the two have been compared.
pub(super) fn difference(outcome: Outcome<'_>, cx: &App) -> AnyElement {
    let difference = match outcome {
        Outcome::Comparing => return placeholder("Comparing…".into(), cx),
        Outcome::Failed => return placeholder("These images couldn't be compared".into(), cx),
        Outcome::Made(difference) => difference.clone(),
    };
    let (colour, apart) = (changed_colour(cx), cx.theme().muted_foreground.opacity(STRIPES));
    let covers = (difference.width, difference.height);
    let drawn = fitted(covers, Placed::Top, move |scale, cx| {
        let at = |side: u32| px(pixels(side) * scale);
        // Across what only one covers, so it reads as set apart rather than as what changed.
        let striped = difference.uncovered.iter().map(|place| {
            div()
                .absolute()
                .left(at(place.x))
                .top(at(place.y))
                .w(at(place.width))
                .h(at(place.height))
                .bg(pattern_slash(apart, 1., 6.))
        });
        let places = difference.places_drawn(outlines_meet_within(scale));
        let outlines = places.iter().map(|place| {
            let off = |side: u32| px(pixels(side) * scale - STAND_OFF);
            let across = |side: u32| px(pixels(side) * scale + 2. * STAND_OFF);
            div()
                .absolute()
                .left(off(place.x))
                .top(off(place.y))
                .w(across(place.width))
                .h(across(place.height))
                .rounded(px(3.))
                .border_1()
                .border_color(colour.opacity(OUTLINE))
                .bg(colour.opacity(WASH))
        });
        let stack = on_checkerboard(difference.picture.clone(), covers, scale, cx).children(striped).children(outlines);
        edged(stack, cx)
    });
    v_flex()
        .id("images-difference")
        .flex_1()
        .min_h_0()
        .w_full()
        .p_6()
        .child(drawn.flex_1().w_full())
        .test_support()
        .into_any_element()
}

/// How far apart, in the image's pixels, two places are when their outlines, standing off them on the screen, would
/// come within [`NEARLY`] of each other with the image drawn at `scale`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "it's positive, and anything past what a u32 holds is as far as it can be, which joins every place"
)]
fn outlines_meet_within(scale: f32) -> u32 {
    ((2. * STAND_OFF + NEARLY) / scale).ceil() as u32
}
