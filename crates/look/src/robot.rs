//! A subagent at work in the background: the robot its call's line has, with its eyes glowing and dimming as a
//! task's ring has its light go round, so its line reads as the call it came from and still says it's at work.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, ParentElement as _, Pixels, Point, Styled as _,
    Window, canvas, div, point, px, quad, size, transparent_black,
};

use crate::clock;
use crate::mark::{BACKGROUND_S, LIGHT, STILL_S, Size, phase};
use crate::palette::faint;

/// How big a call's icon is on its line in the chat, which the robot is too, since it's the icon a subagent's call has.
pub const LINE_ICON: f32 = 15.0;
/// The grid `bot.svg`, in the kit's assets, is drawn on, and where each eye's stroke is on it, as its `M9 13v2` and
/// `M15 13v2` say: upright, two wide, and four tall with its round ends.
const GRID: f32 = 24.0;
const EYES: [(f32, f32); 2] = [(9.0, 14.0), (15.0, 14.0)];
const EYE: (f32, f32) = (2.0, 4.0);
/// How lit its eyes are at their dimmest, so they never look switched off.
const DIMMEST: f32 = 0.25;

/// The robot, as wide as a task's ring, so what follows it lines up with the other lines'.
pub fn robot_at_work(cx: &App) -> impl IntoElement {
    let light = cx.theme().foreground;
    let moves = clock::moving(cx);
    let t = if moves { clock::elapsed() } else { STILL_S };
    let width = Size::Inline.width();
    let inset = (width - px(LINE_ICON)) * 0.5;
    div()
        .relative()
        .flex_none()
        .size(width)
        .child(
            div()
                .absolute()
                .top(inset)
                .left(inset)
                .child(Icon::new(IconName::Bot).size(px(LINE_ICON)).text_color(faint())),
        )
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    let lit = light.opacity(glow(phase(t, BACKGROUND_S)) * LIGHT);
                    for (x, y) in EYES {
                        eye(window, bounds, inset, (x, y), lit);
                    }
                    // Drawn again on the display's next frame, for as long as it's on screen and moving.
                    if moves {
                        window.request_animation_frame();
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size(width),
        )
}

/// How lit the eyes are when the light has `turned` that far round: fully at the moment pictures hold, and never less
/// than [`DIMMEST`].
fn glow(turned: f32) -> f32 {
    let still = phase(STILL_S, BACKGROUND_S);
    let wave = 0.5 + 0.5 * (std::f32::consts::TAU * (turned - still)).cos();
    DIMMEST + (1.0 - DIMMEST) * wave
}

/// One eye, lit, over the icon's own stroke for it, at `(x, y)` on its grid.
fn eye(window: &mut Window, bounds: Bounds<Pixels>, inset: Pixels, (x, y): (f32, f32), color: Hsla) {
    let scale = LINE_ICON / GRID;
    let center = Point::new(bounds.origin.x + inset + px(x * scale), bounds.origin.y + inset + px(y * scale));
    let (w, h) = (px(EYE.0 * scale), px(EYE.1 * scale));
    let stroke = Bounds::new(point(center.x - w * 0.5, center.y - h * 0.5), size(w, h));
    window.paint_quad(quad(
        stroke,
        Corners::all(w * 0.5),
        color,
        Edges::all(px(0.)),
        transparent_black(),
        BorderStyle::default(),
    ));
}

#[cfg(test)]
mod tests {
    use super::{DIMMEST, glow};
    use crate::fan::as_f32;
    use crate::mark::{BACKGROUND_S, STILL_S, phase};

    #[test]
    fn the_eyes_are_fully_lit_in_pictures_and_never_go_dark() {
        let around: Vec<f32> = (0..36).map(|step| glow(as_f32(step) / 36.0)).collect();

        assert!((glow(phase(STILL_S, BACKGROUND_S)) - 1.0).abs() < 1e-5);
        assert!(around.iter().all(|lit| *lit >= DIMMEST - 1e-5 && *lit <= 1.0 + 1e-5), "{around:?}");
        let (dimmest, brightest) = around.iter().fold((1.0_f32, 0.0_f32), |(lo, hi), lit| (lo.min(*lit), hi.max(*lit)));
        assert!(dimmest < 0.3 && brightest > 0.95, "they glow and dim: {dimmest} to {brightest}");
    }
}
