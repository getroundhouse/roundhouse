//! Recording the real app a frame at a time, as a display has it draw, for a clip. Each frame lets a 60th of a second
//! pass on the app's clock, and then, as one change, sends the input that arrived since the frame before, such as a
//! key or a click, and brings what was waiting for this frame, as a display does, so the window draws once for both.
//! So what moves plays back at the speed the app would really run, the same on every run, and what a person does shows
//! first in the frame after it, as it does for them. The exception is the kit's own motion, which keeps the real clock,
//! such as the scrollbar's fade, so the scrollbar's shade can differ a little from one run to the next. GPUI's tests
//! draw a window as soon as something in it changes, so what the time passing changes, such as a reply from the hub,
//! draws as it arrives, as well. Motion is on, unlike for a screen's picture.

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::{HeldKey, TestWindowExt as _};
use gpui_kit::{App, Pixels, PlatformTextSystem, ScrollDelta, Window, point, px};
use image::RgbaImage;

use super::{Driver, id_in, open};
use crate::gallery::steps::{Clip, FPS, Frames, Shot, Target};

/// How many frames a key is held down for, about as long as a person holds one.
const KEY_HELD: Frames = Frames(5);
/// How many frames apart the characters of what's typed arrive, which is 12 a second.
const TYPING: Frames = Frames(5);

/// Something done in a clip.
pub(crate) struct Done {
    /// The first frame it shows in.
    pub(crate) frame: usize,
    pub(crate) said: String,
}

/// Where the pointer was as a frame was drawn, which the system draws, so a window's picture doesn't have it, and how
/// many frames before it last clicked, if it has.
#[derive(Clone, Copy)]
pub(crate) struct Pointer {
    /// From the window's top left, in points.
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The window's pixels to a point.
    pub(crate) scale: f32,
    pub(crate) since_click: Option<Frames>,
}

/// Opens the app on `clip`'s world, does its steps, as a screen does before its picture, and records its shots, handing
/// `film` each frame as it's drawn, with where the pointer was once something in the clip has moved it. Returns what was
/// done in it.
pub(crate) fn record(
    clip: &Clip,
    text_system: &Arc<dyn PlatformTextSystem>,
    film: &mut dyn FnMut(&RgbaImage, Option<Pointer>),
) -> Vec<Done> {
    let (_dir, mut app) = open(Some(clip.world), false, text_system);
    for step in clip.steps {
        app.step(step);
    }
    // What the steps opened shows as it ends, as on a screen, and only what happens in the clip moves.
    let first = app.settle();
    assert!(first.settled, "the screen {} starts from never stopped changing", clip.name);
    app.cx.update(|cx| cx.set_reduce_motion(false));
    film(&first.image, None);
    let mut recorder = Recorder {
        app,
        film,
        frames: 1,
        done: Vec::new(),
        arrived: Vec::new(),
        held: Vec::new(),
        pointed: false,
        since_click: None,
    };
    for shot in clip.shots {
        recorder.shoot(shot);
    }
    assert!(recorder.arrived.is_empty(), "the clip ends with a frame or more after what's done last, to show it");
    recorder.app.close();
    recorder.done
}

/// Input that arrived since the last frame, which the next frame sends.
#[derive(Clone, Copy)]
enum Input {
    KeyDown(&'static str),
    Click(Target),
    Point(Target),
    Scroll(Target, Pixels),
    Type(char),
}

struct Recorder<'a> {
    app: Driver,
    film: &'a mut dyn FnMut(&RgbaImage, Option<Pointer>),
    /// How many frames it has, which is the number of the next.
    frames: usize,
    done: Vec<Done>,
    arrived: Vec<Input>,
    /// The keys down, each with the frame it comes up in.
    held: Vec<(usize, HeldKey)>,
    /// Whether anything in the clip has moved the pointer.
    pointed: bool,
    since_click: Option<Frames>,
}

impl Recorder<'_> {
    fn shoot(&mut self, shot: &Shot) {
        match shot {
            Shot::Hold(frames) => self.hold(*frames),
            Shot::Press(keys) => self.arrive(Input::KeyDown(keys), format!("presses {keys}")),
            Shot::Click(target) => self.arrive(Input::Click(*target), format!("clicks {}", named(*target))),
            Shot::Point(target) => self.arrive(Input::Point(*target), format!("points at {}", named(*target))),
            Shot::Scroll { over, down, frames } => {
                let down = f32::from(*down);
                let way = if down < 0. { "up" } else { "down" };
                let (over, Frames(frames)) = (*over, *frames);
                self.note(format!("scrolls {} points {way} over {}, in {frames} frames", down.abs(), named(over)));
                let share = px(down / f32::from(u16::try_from(frames).unwrap()));
                for _ in 0..frames {
                    self.arrived.push(Input::Scroll(over, share));
                    self.hold(Frames(1));
                }
            }
            Shot::Type(text) => {
                self.note(format!("types {text:?}"));
                for character in text.chars() {
                    self.arrived.push(Input::Type(character));
                    self.hold(TYPING);
                }
            }
        }
    }

    fn arrive(&mut self, input: Input, said: String) {
        self.note(said);
        self.arrived.push(input);
    }

    /// Says what's done, at the frame it first shows in, the next.
    fn note(&mut self, said: String) {
        self.done.push(Done { frame: self.frames, said });
    }

    fn hold(&mut self, Frames(frames): Frames) {
        for _ in 0..frames {
            self.next_frame();
        }
    }

    /// Brings the next frame, as a display does, with what arrived since the one before, and hands it to the film.
    fn next_frame(&mut self) {
        let arrived = std::mem::take(&mut self.arrived);
        let clicked = arrived.iter().any(|input| matches!(input, Input::Click(_)));
        self.pointed |=
            arrived.iter().any(|input| matches!(input, Input::Click(_) | Input::Point(_) | Input::Scroll(..)));
        let (letting_go, held): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.held).into_iter().partition(|(up, _)| *up <= self.frames);
        self.held = held;
        let comes_up = self.frames + usize::try_from(KEY_HELD.0).unwrap();
        let app = &mut self.app;
        app.cx.advance_clock(Duration::from_secs(1) / FPS);
        app.cx.run_until_parked();
        // One update, so the window draws once, as it ends, if anything in it changed.
        let (pressed, at, scale) = app
            .cx
            .update_window(app.handle, |_, window, cx| {
                for (_, key) in letting_go {
                    window.between_frames().key_up(key, cx);
                }
                let pressed: Vec<HeldKey> = arrived.into_iter().filter_map(|input| send(input, window, cx)).collect();
                window.simulate_next_frame(cx);
                (pressed, window.mouse_position(), window.scale_factor())
            })
            .unwrap();
        app.cx.run_until_parked();
        self.held.extend(pressed.into_iter().map(|key| (comes_up, key)));
        self.since_click = if clicked { Some(Frames(0)) } else { self.since_click.map(|Frames(n)| Frames(n + 1)) };
        let pointer = Pointer { x: f32::from(at.x), y: f32::from(at.y), scale, since_click: self.since_click };
        let frame = app.cx.capture_screenshot(app.handle).unwrap();
        (self.film)(&frame, self.pointed.then_some(pointer));
        self.frames += 1;
    }
}

/// Sends `input`, returning the key it held down, if it pressed one.
fn send(input: Input, window: &mut Window, cx: &mut App) -> Option<HeldKey> {
    match input {
        Input::KeyDown(keys) => return Some(window.between_frames().key_down(keys, cx)),
        Input::Click(target) => {
            let id = id_in(window, target);
            window.between_frames().click(id, cx);
        }
        Input::Point(target) => {
            let id = id_in(window, target);
            window.between_frames().hover(id, cx);
        }
        Input::Scroll(target, down) => {
            let id = id_in(window, target);
            // GPUI's wheel scrolls up for more than nothing.
            window.between_frames().scroll(id, ScrollDelta::Pixels(point(px(0.), -down)), cx);
        }
        Input::Type(character) => window.between_frames().type_character(character, cx),
    }
    None
}

/// `target` as what was done says it.
fn named(target: Target) -> String {
    match target {
        Target::Id(id) => format!("#{id}"),
        Target::Label(label) => format!("the line {label:?}"),
    }
}
