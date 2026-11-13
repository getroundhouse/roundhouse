//! A subagent at work in the background: the robot its call's line has, with a white light sweeping across it as the
//! agent's light sweeps its fan, since a subagent is the agent's own work. So its line reads as the call it came from,
//! and says plainly that it's at work.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::{
    App, Bounds, ContentMask, Hsla, IntoElement, ParentElement as _, Pixels, Styled as _, TransformationMatrix, Window,
    canvas, div, point, px, size,
};

use crate::clock;
use crate::colors::colors;
use crate::fan::as_f32;
use crate::mark::{BACKGROUND_S, LIGHT, STILL_S, Size, light_on, phase};

/// How big a call's icon is on its line in the chat, which the robot is too, since it's the icon a subagent's call has.
pub const LINE_ICON: f32 = 15.0;
/// How many strips across the robot the light is laid on in, each lit by how near the light is, so it fades at its
/// edges rather than cutting off. At the line's size, each is a pixel wide.
const STRIPS: usize = 15;

/// The robot, as wide as a task's ring, so what follows it lines up with the other lines'.
pub fn robot_at_work(cx: &App) -> impl IntoElement {
    let colors = colors(cx);
    let (light, faint) = (colors.text, colors.text_faint);
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
                .child(Icon::new(IconName::Bot).size(px(LINE_ICON)).text_color(faint)),
        )
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, cx| {
                    let robot = Bounds::new(bounds.origin + point(inset, inset), size(px(LINE_ICON), px(LINE_ICON)));
                    lit(window, robot, light, sweep(t), cx);
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

        assert_eq!(brightest(STILL_S), Some(STRIPS / 2), "{lit:?}");
        assert!(lit[0] < 0.1 && lit[STRIPS - 1] < 0.1, "its sides are in the dark: {lit:?}");
    }
}
