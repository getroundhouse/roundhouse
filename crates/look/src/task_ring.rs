//! A background task's ring: a small circle of dots around a hub, beside what the agent said it's for. It's the
//! fan's turntable on its own, so the fan stays the agent's, one to a chat. The app's own work in the background, such
//! as a check for updates, has one too.

use gpui_kit::{App, Bounds, Hsla, IntoElement, Pixels, Styled, Window, canvas, point};

use crate::clock;
use crate::colors::colors;
use crate::fan::{self, as_f32};
use crate::mark::{BACKGROUND_S, LIGHT, STILL_S, Size, phase, unlit};

/// Whether a background task, or the app's own work in the background, is doing something.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskRing {
    /// Running, such as a command or a subagent. A white light goes round the ring.
    Running,
    /// Watching for something, as a monitor does. The ring holds still with its hub lit, since nothing is being done.
    Watching,
}

const DOTS: usize = 10;
/// The ring's radius, a dot's and a lit hub's, as shares of the box's width.
const RADIUS: f32 = 0.3;
const DOT: f32 = 0.053;
const LIT_HUB: f32 = 0.082;
/// How much of the ring the light's tail takes, behind the dot it's on.
const TAIL: f32 = 1.0 / 3.0;
/// How strongly a watching ring's dots show, so its hub is what's seen.
const WATCHING: f32 = 0.75;

/// The ring for one background task, or a piece of the app's own work, as wide as a mark beside text, so what follows
/// lines up with the working line's.
pub fn task_ring(ring: TaskRing, cx: &App) -> impl IntoElement {
    let colors = colors(cx);
    let (light, faint) = (colors.text, colors.text_faint);
    let moves = ring == TaskRing::Running && clock::moving(cx);
    let t = if moves { clock::elapsed() } else { STILL_S };
    let width = Size::Inline.width();
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            paint(bounds, ring, faint, light, phase(t, BACKGROUND_S), window);
            // Drawn again on the display's next frame, for as long as it's on screen and moving.
            if moves {
                window.request_animation_frame();
            }
        },
    )
    .flex_none()
    .size(width)
}

fn paint(bounds: Bounds<Pixels>, ring: TaskRing, color: Hsla, light: Hsla, turned: f32, window: &mut Window) {
    let width = bounds.size.width;
    let center = point(bounds.origin.x + width * 0.5, bounds.origin.y + bounds.size.height * 0.5);
    for dot in 0..DOTS {
        let around = as_f32(dot) / as_f32(DOTS);
        let angle = std::f32::consts::TAU * (around - 0.25);
        let at = point(center.x + width * RADIUS * angle.cos(), center.y + width * RADIUS * angle.sin());
        match ring {
            TaskRing::Running => {
                fan::disc(window, at, width * DOT, color);
                let glow = light_on(around, turned);
                if glow > 0.01 {
                    fan::disc(window, at, width * DOT, light.opacity(glow * LIGHT));
                }
            }
            TaskRing::Watching => fan::disc(window, at, width * DOT, color.opacity(WATCHING)),
        }
    }
    match ring {
        TaskRing::Running => fan::disc(window, center, width * DOT, unlit(color)),
        TaskRing::Watching => fan::disc(window, center, width * LIT_HUB, light.opacity(LIGHT)),
    }
}

/// How much of the light is on the dot `around` the ring, from 0 to 1, when the light has `turned` that far: full on
/// the dot under it, fading out behind it, and none ahead.
fn light_on(around: f32, turned: f32) -> f32 {
    let behind = (turned - around).rem_euclid(1.0);
    (1.0 - behind / TAIL).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{DOTS, light_on};
    use crate::fan::as_f32;

    #[test]
    fn the_light_goes_round_with_a_tail_behind_it() {
        let lit = |turned: f32| Vec::from_fn(DOTS, |dot| light_on(as_f32(dot) / as_f32(DOTS), turned));

        let at_the_third = lit(0.3);
        assert!((at_the_third[3] - 1.0).abs() < 1e-5, "{at_the_third:?}");
        assert!(at_the_third[2] > at_the_third[1] && at_the_third[1] > at_the_third[0], "{at_the_third:?}");
        assert!(at_the_third[4..].iter().all(|glow| *glow == 0.0), "none ahead: {at_the_third:?}");
        // Round the top: the tail reaches back past the first dot to the last ones.
        let at_the_first = lit(0.0);
        assert!(at_the_first[0] > at_the_first[9] && at_the_first[9] > at_the_first[8], "{at_the_first:?}");
        assert!(at_the_first[8] > 0.0, "{at_the_first:?}");
    }
}
