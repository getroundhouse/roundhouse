//! The pointer, drawn over a clip's video, since the system draws it rather than the window, so a window's picture
//! doesn't have it: a white ring, edged dark to show on light and dark alike, where it is, filled for a moment after
//! it clicks. The clip's frames are kept without it, as the app drew them.

#![cfg(test)]

use image::{Rgba, RgbaImage};

use crate::gallery::capture::Pointer;
use crate::gallery::frame::to_pixels;
use crate::gallery::steps::Frames;

/// The ring's radius, to its outside, in points.
const RADIUS: f32 = 9.;
/// How wide the ring is, in points.
const RING: f32 = 2.;
/// How wide the dark edge on each side of it is, in points.
const EDGE: f32 = 0.75;
/// How long a click fills the ring for, fading as it goes.
const CLICK_SHOWS: Frames = Frames(15);
/// How much the fill covers what's under it, as a click starts.
const FILL: f32 = 0.6;
/// How much the dark edge covers what's under it.
const DARK: f32 = 0.5;

/// Draws `pointer` over `frame`.
pub(crate) fn draw(frame: &mut RgbaImage, pointer: Pointer) {
    let scale = pointer.scale;
    let (centre_x, centre_y) = (pointer.x * scale, pointer.y * scale);
    let (outer, ring, edge) = (RADIUS * scale, RING * scale, EDGE * scale);
    let inner = outer - ring;
    let clicked = pointer.since_click.filter(|since| *since < CLICK_SHOWS);
    let fill = clicked.map_or(0., |since| FILL * (1. - through(since, CLICK_SHOWS)));
    let reach = outer + edge + 1.;
    let (left, top) = (to_pixels(pointer.x - reach / scale, scale), to_pixels(pointer.y - reach / scale, scale));
    let (right, bottom) = (to_pixels(pointer.x + reach / scale, scale), to_pixels(pointer.y + reach / scale, scale));
    for y in top..bottom.min(frame.height()) {
        for x in left..right.min(frame.width()) {
            let distance = (pixel_centre(x) - centre_x).hypot(pixel_centre(y) - centre_y);
            let dark = DARK * (cover(distance, outer, outer + edge) + cover(distance, inner - edge, inner));
            let white = cover(distance, inner, outer) + fill * cover(distance, 0., inner - edge);
            let pixel = frame.get_pixel_mut(x, y);
            *pixel = over(over(*pixel, [0., 0., 0.], dark.min(1.)), [255., 255., 255.], white.min(1.));
        }
    }
}

/// How much of a pixel whose centre is `distance` from the pointer's lies between `from` and `to`, softened over a
/// pixel so the ring's edges aren't jagged.
fn cover(distance: f32, from: f32, to: f32) -> f32 {
    (distance - from + 0.5).clamp(0., 1.) * (to - distance + 0.5).clamp(0., 1.)
}

/// How far `part` is through `whole`.
fn through(Frames(part): Frames, Frames(whole): Frames) -> f32 {
    let float = |frames: u32| f32::from(u16::try_from(frames).unwrap());
    float(part) / float(whole)
}

/// `colour` laid over `pixel`, covering `amount` of it.
fn over(pixel: Rgba<u8>, colour: [f32; 3], amount: f32) -> Rgba<u8> {
    let [r, g, b, a] = pixel.0;
    let mix = |under: u8, over: f32| byte(f32::from(under) * (1. - amount) + over * amount);
    Rgba([mix(r, colour[0]), mix(g, colour[1]), mix(b, colour[2]), a])
}

#[expect(clippy::cast_precision_loss, reason = "a pixel's place in a frame, which f32 holds exactly")]
fn pixel_centre(at: u32) -> f32 {
    at as f32 + 0.5
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to a byte's range first")]
fn byte(value: f32) -> u8 {
    value.round().clamp(0., 255.) as u8
}
