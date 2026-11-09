//! The app icon: the fan in copper on a warm dark tile, with a drawing of its own for each size macOS keeps one for.
//! The gallery draws it into `apps/desktop/AppIcon.iconset/`, which the app is packaged with, and checks it there.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, Pixels, Styled, Window, canvas, fill,
    linear_color_stop, linear_gradient, point, px, quad, size,
};

use crate::fan::{self, ASPECT, Count, Fan};
use crate::palette::tile_top;

/// The sizes macOS keeps a drawing of the icon for. It picks one by the pixels it needs, so on a Retina display
/// each is shown at half its pixels in points, and the 128 is what the Dock shows at 64 points.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconSize {
    Px16,
    Px32,
    Px64,
    Px128,
    Px256,
    Px512,
    Px1024,
}

impl IconSize {
    pub const ALL: [IconSize; 7] = [
        IconSize::Px16,
        IconSize::Px32,
        IconSize::Px64,
        IconSize::Px128,
        IconSize::Px256,
        IconSize::Px512,
        IconSize::Px1024,
    ];

    pub fn pixels(self) -> u16 {
        match self {
            IconSize::Px16 => 16,
            IconSize::Px32 => 32,
            IconSize::Px64 => 64,
            IconSize::Px128 => 128,
            IconSize::Px256 => 256,
            IconSize::Px512 => 512,
            IconSize::Px1024 => 1024,
        }
    }

    /// Each is tuned by eye at its size, with fewer rows as it shrinks so the dots stay big enough to see: the Dock
    /// shrinks the 256 to under half. Only the biggest have the pixels for nine rows of fine dots. Below 128 the fan's
    /// own shape would blur, so it's drawn on whole pixels instead.
    fn drawing(self) -> Drawing {
        match self {
            IconSize::Px16 => {
                Drawing::OnPixels(OnPixels { inset: 1., rows: 2, dot: 1., pitch: 2.5, inner: 3., hub: 2., ring: None })
            }
            IconSize::Px32 => {
                Drawing::OnPixels(OnPixels { inset: 3., rows: 2, dot: 2., pitch: 4., inner: 5.5, hub: 2., ring: None })
            }
            IconSize::Px64 => Drawing::OnPixels(OnPixels {
                inset: 6.,
                rows: 3,
                dot: 2.,
                pitch: 4.5,
                inner: 8.,
                hub: 2.,
                ring: Some(4.),
            }),
            IconSize::Px128 => Drawing::Scaled { rows: 3, fan: 0.66 },
            IconSize::Px256 => Drawing::Scaled { rows: 4, fan: 0.66 },
            IconSize::Px512 | IconSize::Px1024 => Drawing::Scaled { rows: 9, fan: 0.62 },
        }
    }
}

enum Drawing {
    /// The fan's own shape, `fan` of the canvas wide.
    Scaled {
        rows: usize,
        fan: f32,
    },
    OnPixels(OnPixels),
}

/// A fan laid out in the display's pixels, on a tile `inset` from the edge: each dot `dot` across, rows `pitch`
/// apart from `inner` out, and dots about `pitch` apart along a row. The turntable is a hub `hub` across, in a ring of
/// one pixel out to `ring`, if any. The tile's middle falls between two pixels, so the hub and the ring are an even
/// number of pixels across, to centre on it.
struct OnPixels {
    inset: f32,
    rows: usize,
    dot: f32,
    pitch: f32,
    inner: f32,
    hub: f32,
    ring: Option<f32>,
}

/// macOS's grid for an app icon: the tile is inset by this share of the canvas, and rounded by this share of itself.
/// macOS masks the icon to its own shape anyway, so the tile only has to sit under that mask.
const INSET: f32 = 100. / 1024.;
const CORNER: f32 = 0.225;
/// The tile's edge, as a share of the canvas.
const EDGE: f32 = 0.003;
/// How much a turntable's hub and ring show, against the dots.
const TURNTABLE: f32 = 0.6;

/// The icon at `size`, drawn for a Retina display, so it's half as many points wide as its drawing has pixels.
pub fn icon(size: IconSize, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let colors = Colors { copper: theme.ring, top: tile_top(), bottom: theme.background, edge: theme.border };
    let width = px(f32::from(size.pixels()) / 2.0);
    canvas(|_, _, _| {}, move |bounds, (), window, _| paint(bounds, &size.drawing(), &colors, window))
        .flex_none()
        .w(width)
        .h(width)
}

/// Copper is the theme's focus ring, as for the brand, and the tile fades down to the pane's colour.
struct Colors {
    copper: Hsla,
    top: Hsla,
    bottom: Hsla,
    edge: Hsla,
}

fn paint(bounds: Bounds<Pixels>, drawing: &Drawing, colors: &Colors, window: &mut Window) {
    let scale = window.scale_factor();
    let side = bounds.size.width;
    let inset = match drawing {
        Drawing::Scaled { .. } => side * INSET,
        Drawing::OnPixels(fan) => px(fan.inset / scale),
    };
    let tile = Bounds::new(
        point(bounds.origin.x + inset, bounds.origin.y + inset),
        size(side - inset * 2.0, side - inset * 2.0),
    );
    let fill = linear_gradient(180., linear_color_stop(colors.top, 0.), linear_color_stop(colors.bottom, 1.));
    let corners = Corners::all(tile.size.width * CORNER);
    window.paint_quad(quad(tile, corners, fill, Edges::all(side * EDGE), colors.edge, BorderStyle::default()));
    match drawing {
        Drawing::Scaled { rows, fan } => scaled(bounds, *rows, side * *fan, colors.copper, window),
        Drawing::OnPixels(fan) => on_pixels(bounds, fan, colors.copper, window),
    }
}

/// Centred on what it reaches rather than on its box, which leaves room under the turntable. Every row has a dot on
/// the middle line, so the rows line up down it rather than staggering.
fn scaled(bounds: Bounds<Pixels>, rows: usize, width: Pixels, copper: Hsla, window: &mut Window) {
    let placed = Fan::with_rows(Bounds::new(point(px(0.), px(0.)), size(width, width * ASPECT)), rows);
    let (up, down) = placed.reach();
    let middle = bounds.center();
    let center = point(middle.x, middle.y + (up - down) * 0.5);
    let fan = Fan::with_rows(Bounds::new(center - placed.center, size(width, width * ASPECT)), rows);
    for dot in fan.dots_counted(Count::Odd) {
        fan::disc(window, dot.at, fan.dot, copper);
    }
    let turntable = fan.turntable();
    fan::ring(window, fan.center, turntable.radius, turntable.line, copper.opacity(TURNTABLE));
    fan::disc(window, fan.center, turntable.hub, copper.opacity(TURNTABLE));
}

/// Each dot is a square of whole pixels, which at this size is sharper than a circle's soft edge.
fn on_pixels(bounds: Bounds<Pixels>, fan: &OnPixels, copper: Hsla, window: &mut Window) {
    let scale = window.scale_factor();
    let layout = Layout::of(fan, bounds.size.width * scale / px(1.));
    let at = |(x, y): (f32, f32)| point(bounds.origin.x + px(x / scale), bounds.origin.y + px(y / scale));
    let side = px(fan.dot / scale);
    for dot in &layout.dots {
        let corner = at(*dot);
        let square = Bounds::new(point(corner.x - side * 0.5, corner.y - side * 0.5), size(side, side));
        window.paint_quad(fill(square, copper));
    }
    let turntable = copper.opacity(TURNTABLE);
    if let Some(ring) = fan.ring {
        fan::ring(window, at(layout.center), px(ring / scale), px(1. / scale), turntable);
    }
    fan::disc(window, at(layout.center), px(fan.hub / 2.0 / scale), turntable);
}

/// Where a fan on whole pixels goes, in pixels from the canvas's corner: its centre, and its dots' centres.
struct Layout {
    center: (f32, f32),
    dots: Vec<(f32, f32)>,
}

impl Layout {
    /// A dot that would touch one already placed is left out, since two touching dots read as one blot. The left
    /// half is placed and the right mirrors it, so a dot left out on one side is left out on the other too.
    fn of(fan: &OnPixels, canvas: f32) -> Self {
        let outer = fan.inner + fan.pitch * fan::as_f32(fan.rows - 1);
        let (up, down) = (outer + fan.dot / 2.0, (fan.hub / 2.0).max(fan.ring.unwrap_or(0.)));
        // On the line between the tile's two middle pixels, and centred up and down on what it reaches. Every row has
        // a dot on that line, as in `scaled`, except where dots are an odd number of pixels across: they sit on
        // pixels' middles, so to mirror across the line, none can sit on it, and every row has a gap there instead.
        let center = (canvas / 2.0, ((canvas + up - down) / 2.0).round());
        let count = if odd(fan.dot) { Count::Even } else { Count::Odd };
        let touch = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs().max((a.1 - b.1).abs()) < fan.dot + 1.;
        let mut dots: Vec<(f32, f32)> = Vec::new();
        let middle = point(px(center.0), px(center.1));
        let rows = fan::rows_of_dots(middle, px(fan.inner), px(fan.pitch), fan.rows, count);
        for dot in rows.filter(|d| d.along <= 0.5) {
            let at = (on_whole_pixels(dot.at.x / px(1.), fan.dot), on_whole_pixels(dot.at.y / px(1.), fan.dot));
            let twins = if dot.along < 0.5 { vec![at, (center.0 * 2.0 - at.0, at.1)] } else { vec![at] };
            let crowded = twins.iter().any(|t| dots.iter().any(|d| touch(*t, *d)));
            if crowded || (twins.len() == 2 && touch(twins[0], twins[1])) {
                continue;
            }
            dots.extend(twins);
        }
        Layout { center, dots }
    }
}

/// Where something `across` pixels wide covers whole pixels: centred on a pixel's edge if it's an even number of
/// pixels across, and on a pixel's middle if odd.
fn on_whole_pixels(v: f32, across: f32) -> f32 {
    if odd(across) { v.floor() + 0.5 } else { v.round() }
}

fn odd(pixels: f32) -> bool {
    (pixels / 2.0).fract() > 0.25
}

#[cfg(test)]
mod tests {
    use super::{Drawing, IconSize, Layout, OnPixels, odd};

    fn on_pixels() -> Vec<(u16, OnPixels)> {
        IconSize::ALL
            .into_iter()
            .filter_map(|size| match size.drawing() {
                Drawing::OnPixels(fan) => Some((size.pixels(), fan)),
                Drawing::Scaled { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_smallest_sizes_are_drawn_on_whole_pixels() {
        let sizes: Vec<u16> = on_pixels().into_iter().map(|(pixels, _)| pixels).collect();

        assert_eq!(sizes, [16, 32, 64]);
    }

    #[test]
    fn every_dot_on_whole_pixels_has_a_pixel_clear_around_it_and_sits_on_the_tile() {
        for (pixels, fan) in on_pixels() {
            let canvas = f32::from(pixels);
            let layout = Layout::of(&fan, canvas);

            assert!(layout.dots.len() > 8, "{pixels}: only {} dots", layout.dots.len());
            for (i, a) in layout.dots.iter().enumerate() {
                for b in &layout.dots[i + 1..] {
                    let apart = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                    assert!(apart >= fan.dot + 1., "{pixels}: dots at {a:?} and {b:?} touch");
                }
                let reach = (a.0 - fan.dot / 2.0, a.0 + fan.dot / 2.0, a.1 - fan.dot / 2.0, a.1 + fan.dot / 2.0);
                let tile = (fan.inset, canvas - fan.inset);
                assert!(reach.0 >= tile.0 && reach.1 <= tile.1 && reach.2 >= tile.0 && reach.3 <= tile.1, "{pixels}");
            }
        }
    }

    #[test]
    fn a_fan_on_whole_pixels_is_the_same_on_both_sides() {
        for (pixels, fan) in on_pixels() {
            let layout = Layout::of(&fan, f32::from(pixels));
            let mirrored = |d: &(f32, f32)| (layout.center.0 * 2.0 - d.0, d.1);

            for dot in &layout.dots {
                assert!(layout.dots.contains(&mirrored(dot)), "{pixels}: {dot:?} has no twin");
            }
        }
    }

    #[test]
    fn a_fan_on_whole_pixels_sits_in_the_middle_of_its_tile() {
        for (pixels, fan) in on_pixels() {
            let canvas = f32::from(pixels);
            let layout = Layout::of(&fan, canvas);
            let left = layout.dots.iter().map(|d| d.0 - fan.dot / 2.0).fold(f32::MAX, f32::min);
            let right = layout.dots.iter().map(|d| d.0 + fan.dot / 2.0).fold(f32::MIN, f32::max);

            let (space_left, space_right) = (left - fan.inset, canvas - fan.inset - right);
            assert!((space_left - space_right).abs() < 0.01, "{pixels}: {space_left} on the left, {space_right} right");
            assert!((layout.center.0 - canvas / 2.0).abs() < 0.01, "{pixels}: the turntable");
        }
    }

    #[test]
    fn every_row_of_a_fan_on_whole_pixels_has_the_same_middle() {
        for (pixels, fan) in on_pixels() {
            let layout = Layout::of(&fan, f32::from(pixels));
            let on_the_line = layout.dots.iter().filter(|d| (d.0 - layout.center.0).abs() < 0.01).count();

            let expected = if odd(fan.dot) { 0 } else { fan.rows };
            assert_eq!(on_the_line, expected, "{pixels}: dots on the middle line");
        }
    }

    #[test]
    fn the_turntable_centres_on_the_line_between_two_pixels() {
        for (pixels, fan) in on_pixels() {
            assert!(!odd(fan.hub), "{pixels}: the hub is {} across", fan.hub);
            if let Some(ring) = fan.ring {
                assert!(!odd(ring * 2.0), "{pixels}: the ring is {} across", ring * 2.0);
            }
        }
    }
}
