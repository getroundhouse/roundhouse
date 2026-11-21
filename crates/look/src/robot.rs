//! A subagent at work in the background: the robot its call's line has, with a white light sweeping across it as the
//! agent's light sweeps its fan, since a subagent is the agent's own work. So its line reads as the call it came from,
//! and says plainly that it's at work.

use gpui_kit::assets::IconName;
use gpui_kit::{
    App, Bounds, ContentMask, Hsla, IntoElement, Pixels, TransformationMatrix, Window, canvas, point, size,
};

use crate::clock;
use crate::colors::colors;
use crate::fan::as_f32;
use crate::icon_sizes::IconSize;
use crate::mark::{BACKGROUND_S, LIGHT, STILL_S, light_on, phase};

/// How big the robot is: a call's icon on its line in the chat, since it's the icon a subagent's call has.
const ROBOT: IconSize = IconSize::Large;
/// How many strips across the robot the light is laid on in, each lit by how near the light is, so it fades at its
/// edges rather than cutting off. At the robot's size, each is a pixel wide.
const STRIPS: usize = 16;

/// The robot, as wide as a task's ring, so what follows it lines up with the other lines'.
pub fn robot_at_work(cx: &App) -> impl IntoElement {
    let t = clock::moving(cx).then(|| clock::elapsed(cx));
    let drawn = canvas(
        |_, _, _| {},
        move |bounds, (), window, cx| {
            // Drawn again on the display's next frame, for as long as it's on screen and moving.
            if paint_at(t, bounds, window, cx) {
                window.request_animation_frame();
            }
        },
    );
    crate::task_ring::sized(drawn)
}

/// The robot at work, painted within `bounds`, as [`robot_at_work`] draws it. Returns whether it moves, so what paints
/// it asks for the next frame.
pub(crate) fn paint_robot(bounds: Bounds<Pixels>, window: &mut Window, cx: &App) -> bool {
    paint_at(clock::moving(cx).then(|| clock::elapsed(cx)), bounds, window, cx)
}

/// Paints the robot, in the middle of `bounds`, with the light where it is at `t` seconds, `None` while it holds
/// still, and returns whether it moves.
fn paint_at(t: Option<f64>, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) -> bool {
    let colors = colors(cx);
    let inset = (bounds.size.width - ROBOT.size()) * 0.5;
    let robot = Bounds::new(bounds.origin + point(inset, inset), size(ROBOT.size(), ROBOT.size()));
    // It only fails if the icon can't be read, which leaves the line without its robot, as an icon would be.
    let _ = window.paint_svg(robot, IconName::Bot.path(), None, TransformationMatrix::unit(), colors.text_faint, cx);
    lit(window, robot, colors.text, sweep(t.unwrap_or(STILL_S)), cx);
    t.is_some()
}

/// Where the light is across the robot at `t` seconds, from its left edge at 0 to its right at 1. It starts and ends
/// a little beyond each, so it comes on and goes off gradually, and it's over the robot's face in pictures.
fn sweep(t: f64) -> f32 {
    let from_still = (phase(t, BACKGROUND_S) - phase(STILL_S, BACKGROUND_S) + 0.5).rem_euclid(1.0);
    from_still * 1.5 - 0.25
}

/// How much of the light is on each strip across the robot, from 0 to 1, when it's at `sweep`, as a dot along the fan
/// has it.
fn strips(sweep: f32) -> impl Iterator<Item = f32> {
    (0..STRIPS).map(move |strip| light_on((as_f32(strip) + 0.5) / as_f32(STRIPS), 0, sweep, 0.0))
}

/// The robot again, in the light, over the one beneath it, each strip as lit as [`strips`] says.
fn lit(window: &mut Window, robot: Bounds<Pixels>, light: Hsla, sweep: f32, cx: &App) {
    // Each strip's edges are the next's, worked out the same way, so no column of pixels is lit twice or missed.
    let edge = |strip: usize| robot.origin.x + robot.size.width * (as_f32(strip) / as_f32(STRIPS));
    for (strip, glow) in strips(sweep).enumerate().filter(|(_, glow)| *glow > 0.01) {
        let bounds = Bounds::from_corners(point(edge(strip), robot.top()), point(edge(strip + 1), robot.bottom()));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            // It only fails if the icon can't be read, and the robot beneath shows the same then.
            let _ = window.paint_svg(
                robot,
                IconName::Bot.path(),
                None,
                TransformationMatrix::unit(),
                light.opacity(glow * LIGHT),
                cx,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{STRIPS, strips, sweep};
    use crate::mark::{BACKGROUND_S, STILL_S};

    /// The strip the light is brightest on at `t` seconds, if it's on the robot.
    fn brightest(t: f64) -> Option<usize> {
        let lit: Vec<f32> = strips(sweep(t)).collect();
        let (strip, glow) = lit.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
        (*glow > 0.5).then_some(strip)
    }

    #[test]
    fn the_light_sweeps_across_the_robot_from_left_to_right() {
        let start = STILL_S + BACKGROUND_S * 0.5;
        let along: Vec<Option<usize>> =
            (0..24).map(|step| brightest(start + BACKGROUND_S * f64::from(step) / 24.0)).collect();
        let on: Vec<usize> = along.iter().flatten().copied().collect();

        assert!(on.windows(2).all(|pair| pair[0] <= pair[1]), "left to right: {along:?}");
        assert_eq!((on.first(), on.last()), (Some(&0), Some(&(STRIPS - 1))), "edge to edge: {along:?}");
    }

    #[test]
    fn pictures_catch_the_light_across_its_face() {
        let lit: Vec<f32> = strips(sweep(STILL_S)).collect();

        // Lit alike either side of the middle, which with an even number of strips is between two of them.
        let mirrored = lit.iter().zip(lit.iter().rev()).all(|(left, right)| (left - right).abs() < 1e-3);
        assert!(mirrored, "the light is in the middle of its face: {lit:?}");
        assert!(lit[STRIPS / 2] > 0.5, "lit across the middle: {lit:?}");
        assert!(lit[0] < 0.1 && lit[STRIPS - 1] < 0.1, "its sides are in the dark: {lit:?}");
    }
}
