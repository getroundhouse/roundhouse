use super::pixels;
use crate::fan;
use crate::fan::{ASPECT, Fan};
use crate::mark::{LIGHT, light_on};
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, Pixels, Styled, Window, canvas, fill,
    linear_color_stop, linear_gradient, point, px, quad, size,
};
use pixels::{Layout, OnPixels};

/// The two ways the icon can look. The app's own is `Working`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconStyle {
    /// The fan at rest, as the brand is drawn in an empty state.
    Idle,
    /// The fan caught working, with the agent's white light partway across it, held still where a working mark has
    /// it moving.
    Working,
}

impl IconStyle {
    pub const ALL: [IconStyle; 2] = [IconStyle::Idle, IconStyle::Working];
}

/// The sizes macOS keeps a drawing of the icon for. It mostly picks the smallest with the pixels it needs, so on a
/// Retina display each is shown at up to half its pixels in points. The Dock is the exception, and uses the 256.
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
    /// own shape would blur, so it's drawn on whole pixels instead, with the dots as far apart as keeps a pixel clear
    /// around each.
    pub(super) fn drawing(self) -> Drawing {
        match self {
            IconSize::Px16 => {
                Drawing::OnPixels(OnPixels { inset: 1., rows: 2, dot: 1., spacing: 2.5, hub: 2., ring: None })
            }
            IconSize::Px32 => {
                Drawing::OnPixels(OnPixels { inset: 3., rows: 2, dot: 2., spacing: 3.7, hub: 2., ring: None })
            }
            IconSize::Px64 => {
                Drawing::OnPixels(OnPixels { inset: 6., rows: 3, dot: 2., spacing: 4.2, hub: 2., ring: Some(4.) })
            }
            IconSize::Px128 => Drawing::Scaled { rows: 3, fan: 0.66 },
            IconSize::Px256 => Drawing::Scaled { rows: 4, fan: 0.66 },
            IconSize::Px512 | IconSize::Px1024 => Drawing::Scaled { rows: 9, fan: 0.62 },
        }
    }
}

pub(super) enum Drawing {
    /// The fan's own shape, `fan` of the canvas wide.
    Scaled {
        rows: usize,
        fan: f32,
    },
    OnPixels(OnPixels),
}

/// macOS's grid for an app icon: the tile is inset by this share of the canvas, and rounded by this share of itself.
/// macOS masks the icon to its own shape anyway, so the tile only has to sit under that mask.
const INSET: f32 = 100. / 1024.;
const CORNER: f32 = 0.225;
/// The tile's edge, as a share of the canvas.
const EDGE: f32 = 0.003;
/// How much a turntable's hub and ring show, against the dots.
const TURNTABLE: f32 = 0.6;
/// Where a working icon has the agent's white light, as a working mark draws it: just right of the middle on the inner
/// row, with the outer row trailing as far behind as a four-row mark's, whatever the rows.
const SWEEP: f32 = 0.66;
const TRAIL: f32 = 0.18;

/// The icon in `style` at `size`, drawn for a Retina display, so it's half as many points wide as its drawing has
/// pixels.
pub fn icon(style: IconStyle, size: IconSize, cx: &App) -> impl IntoElement {
    let app = crate::colors::colors(cx);
    let colors =
        Colors { copper: app.brand, light: app.text, top: app.icon_tile_top, bottom: app.pane, edge: app.edge };
    let width = px(f32::from(size.pixels()) / 2.0);
    canvas(|_, _, _| {}, move |bounds, (), window, _| paint(bounds, style, &size.drawing(), &colors, window))
        .flex_none()
        .w(width)
        .h(width)
}

/// Copper is the theme's focus ring, as for the brand, the light is the mark's, and the tile fades down to the pane's
/// colour.
struct Colors {
    pub(super) copper: Hsla,
    pub(super) light: Hsla,
    pub(super) top: Hsla,
    pub(super) bottom: Hsla,
    pub(super) edge: Hsla,
}

fn paint(bounds: Bounds<Pixels>, style: IconStyle, drawing: &Drawing, colors: &Colors, window: &mut Window) {
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
        Drawing::Scaled { rows, fan } => scaled(bounds, style, *rows, side * *fan, colors, window),
        Drawing::OnPixels(fan) => on_pixels(bounds, style, fan, colors, window),
    }
}

/// In the middle of the tile, as a fan is in the middle of its box.
fn scaled(bounds: Bounds<Pixels>, style: IconStyle, rows: usize, width: Pixels, colors: &Colors, window: &mut Window) {
    let fan = Fan::with_rows(Bounds::centered_at(bounds.center(), size(width, width * ASPECT)), rows);
    for dot in fan.dots() {
        fan::disc(window, dot.at, fan.dot, colors.copper);
        let glow = light(style, dot.along, dot.row, rows);
        if glow > 0.01 {
            fan::disc(window, dot.at, fan.dot, colors.light.opacity(glow * LIGHT));
        }
    }
    let turntable = fan.turntable();
    let hub = colors.copper.opacity(TURNTABLE);
    fan::ring(window, fan.center, turntable.radius, turntable.line, hub);
    fan::disc(window, fan.center, turntable.hub, hub);
}

/// Each dot is a square of whole pixels, which at this size is sharper than a circle's soft edge.
fn on_pixels(bounds: Bounds<Pixels>, style: IconStyle, fan: &OnPixels, colors: &Colors, window: &mut Window) {
    let scale = window.scale_factor();
    let layout = Layout::of(fan, bounds.size.width * scale / px(1.));
    let at = |(x, y): (f32, f32)| point(bounds.origin.x + px(x / scale), bounds.origin.y + px(y / scale));
    let side = px(fan.dot / scale);
    for dot in &layout.dots {
        let middle = at(dot.at);
        let square = Bounds::new(point(middle.x - side * 0.5, middle.y - side * 0.5), size(side, side));
        window.paint_quad(fill(square, colors.copper));
        let glow = light(style, dot.along, dot.row, fan.rows);
        if glow > 0.01 {
            window.paint_quad(fill(square, colors.light.opacity(glow * LIGHT)));
        }
    }
    let turntable = colors.copper.opacity(TURNTABLE);
    if let Some(ring) = fan.ring {
        fan::ring(window, at(layout.center), px(ring / scale), px(1. / scale), turntable);
    }
    fan::disc(window, at(layout.center), px(fan.hub / 2.0 / scale), turntable);
}

/// How much of the agent's light is on a dot `along` the fan in `row`, of `rows`, in an icon in `style`.
fn light(style: IconStyle, along: f32, row: usize, rows: usize) -> f32 {
    match style {
        IconStyle::Idle => 0.0,
        IconStyle::Working => light_on(along, row, SWEEP, TRAIL / fan::as_f32(rows - 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::{IconStyle, SWEEP, TRAIL, light};

    #[test]
    fn the_working_icon_has_the_light_just_right_of_the_middle_with_the_outer_row_trailing() {
        for rows in 2..=9 {
            let outer = rows - 1;

            let at = |along| light(IconStyle::Working, along, 0, rows);
            assert!(at(0.5 + 0.1) > at(0.5), "{rows} rows: brighter just right of the middle");
            assert!((light(IconStyle::Working, SWEEP, 0, rows) - 1.0).abs() < 1e-6, "{rows} rows: the inner row");
            assert!((light(IconStyle::Working, SWEEP - TRAIL, outer, rows) - 1.0).abs() < 1e-6, "{rows} rows: outer");
            assert!(light(IconStyle::Working, SWEEP, outer, rows) < 1.0, "{rows} rows: the outer row trails");
            assert!(light(IconStyle::Working, 0.0, 0, rows) < 0.01, "{rows} rows: the left end is dark");
        }
    }

    #[test]
    fn the_idle_icon_has_no_light() {
        for rows in 2..=9 {
            for along in [0.0, 0.25, 0.5, SWEEP, 1.0] {
                for row in 0..rows {
                    assert!(light(IconStyle::Idle, along, row, rows).abs() < f32::EPSILON, "{rows} rows, row {row}");
                }
            }
        }
    }
}
