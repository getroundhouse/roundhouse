use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect { left, top, right: left + width, bottom: top + height }
}

/// The frame GPUI takes off a window's rect, in pixels: left, top, right and bottom.
type Frame = (i32, i32, i32, i32);

/// What GPUI's `calculate_client_rect` gives a window placed at `rect`.
fn gpui_bounds(rect: Rect, (left, top, right, bottom): Frame, scale: f32) -> Bounds<Pixels> {
    let (width, height) = rect.size();
    #[expect(clippy::cast_precision_loss, reason = "a display's pixels are far below where an f32 loses whole ones")]
    let points = |pixels: i32| px(pixels as f32 / scale);
    Bounds::new(
        point(points(rect.left + left), points(rect.top + top)),
        size(points(width - left - right), points(height - top - bottom)),
    )
}

fn a_rect() -> impl Strategy<Value = Rect> {
    (-4000..4000, -2000..2000, 200..4000, 200..3000)
        .prop_map(|(left, top, width, height)| rect(left, top, width, height))
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Whatever the frame, the display's scale and where the window goes, moving GPUI's bounds by how far the window
    /// moved gives the bounds GPUI gives a window placed where it went.
    #[test]
    fn moving_gpuis_bounds_gives_what_gpui_gives_where_the_window_went(
        placed_at in a_rect(),
        now in a_rect(),
        frame in (0..20, 0..20, 0..20, 0..20),
        scale in prop::sample::select(vec![1_f32, 1.25, 1.5, 1.75, 2., 2.5, 3.]),
    ) {
        let moved = moved_to(gpui_bounds(placed_at, frame, scale), placed_at, now, scale);

        prop_assert_eq!(moved.to_device_pixels(scale), gpui_bounds(now, frame, scale).to_device_pixels(scale));
    }
}

/// Stretched to the height of a display at 150%, above a taskbar 72 pixels tall, from 860 points tall halfway down,
/// as measured on Windows 11, where GPUI takes 11 pixels off each side of the window's rect, 5 off its top and 6 off
/// its bottom.
const STRETCHED: (Rect, Rect) =
    (Rect { left: 654, top: 394, right: 3415, bottom: 1695 }, Rect { left: 654, top: 0, right: 3415, bottom: 2097 });
const STRETCHED_FRAME: Frame = (11, 5, 11, 6);

#[test]
fn a_window_stretched_to_its_displays_height_is_where_it_is_rather_than_where_it_goes_back_to() {
    let (placed_at, now) = STRETCHED;
    let goes_back_to = gpui_bounds(placed_at, STRETCHED_FRAME, 1.5);

    let there = bounds(WindowBounds::Windowed(goes_back_to), Shown::Normally { placed_at, now }, 1.5);

    let Some(WindowBounds::Windowed(there)) = there else { panic!("{there:?}") };
    let pixels = there.to_device_pixels(1.5);
    assert_eq!((pixels.origin.x.0, pixels.origin.y.0, pixels.size.width.0, pixels.size.height.0), (665, 5, 2739, 2086));
}

#[test]
fn a_minimized_window_has_no_bounds_however_it_goes_back() {
    let usual = gpui_bounds(rect(100, 100, 1320, 860), STRETCHED_FRAME, 1.);

    for gpui in [WindowBounds::Windowed(usual), WindowBounds::Maximized(usual), WindowBounds::Fullscreen(usual)] {
        assert_eq!(bounds(gpui, Shown::Minimized, 1.), None, "{gpui:?}");
    }
}

#[test]
fn a_window_windows_has_destroyed_has_no_bounds_rather_than_those_gpui_makes_up() {
    // What GPUI gives as Windows fails to read the window's placement, measured as the app quit.
    let made_up = gpui_bounds(rect(0, 0, 0, 0), STRETCHED_FRAME, 1.5);

    assert_eq!(bounds(WindowBounds::Windowed(made_up), Shown::Gone, 1.5), None);
}

#[test]
fn a_window_maximized_or_in_full_screen_keeps_gpuis_bounds() {
    let (placed_at, now) = STRETCHED;
    let goes_back_to = gpui_bounds(placed_at, STRETCHED_FRAME, 1.5);

    for gpui in [WindowBounds::Maximized(goes_back_to), WindowBounds::Fullscreen(goes_back_to)] {
        assert_eq!(bounds(gpui, Shown::Normally { placed_at, now }, 1.5), Some(gpui));
        assert_eq!(bounds(gpui, Shown::Otherwise, 1.5), Some(gpui));
    }
}

#[test]
fn a_window_at_its_placements_size_keeps_gpuis_bounds_even_where_its_rect_isnt_its_placements() {
    // As a window would be, if a placement's terms weren't what `in_placement_terms` takes them to be.
    let placed_at = rect(100, 100, 1320, 860);
    let elsewhere = rect(148, 100, 1320, 860);
    let gpui = WindowBounds::Windowed(gpui_bounds(placed_at, STRETCHED_FRAME, 1.25));

    assert_eq!(bounds(gpui, Shown::Normally { placed_at, now: placed_at }, 1.25), Some(gpui));
    assert_eq!(bounds(gpui, Shown::Normally { placed_at, now: elsewhere }, 1.25), Some(gpui));
    assert_eq!(bounds(gpui, Shown::Otherwise, 1.25), Some(gpui));
}

#[test]
fn a_placement_leaves_out_a_taskbar_along_the_top_or_left_of_the_windows_display() {
    let main = rect(0, 0, 2560, 1440);
    let on_main = rect(48, 40, 1320, 860);
    let beside = rect(2560, 0, 1920, 1080);
    let on_beside = rect(2600, 40, 1320, 860);

    let left_taskbar = rect(48, 0, 2512, 1440);
    let top_taskbar = rect(0, 40, 2560, 1400);
    let bottom_taskbar = rect(0, 0, 2560, 1392);
    let beside_left_taskbar = rect(2608, 0, 1872, 1080);

    assert_eq!(in_placement_terms(on_main, left_taskbar, main), rect(0, 40, 1320, 860));
    assert_eq!(in_placement_terms(on_main, top_taskbar, main), rect(48, 0, 1320, 860));
    assert_eq!(in_placement_terms(on_main, bottom_taskbar, main), on_main);
    assert_eq!(in_placement_terms(on_beside, beside, beside), on_beside);
    assert_eq!(in_placement_terms(on_beside, beside_left_taskbar, beside), rect(2552, 40, 1320, 860));
}
