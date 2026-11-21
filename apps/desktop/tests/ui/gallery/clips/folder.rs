//! Recording a clip into its folder in `.context/clips/`, as `clips` says. Each frame goes into the video, and into
//! `frames/` if it differs from the one before, as it's drawn, so no more than one is kept. The clip is recorded into a
//! folder beside its own, which takes its place once it's all written, so a recording that fails leaves the last one.

#![cfg(test)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use image::RgbaImage;

use super::pointer;
use super::video::Video;
use crate::gallery::capture::{Done, Pointer, record};
use crate::gallery::steps::{Clip, FPS};

/// Records `clip`, and writes it to its folder, in place of what's there.
pub(super) fn record_into_its_folder(clip: &Clip) {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let clips = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/clips");
    let (dir, recording) = (clips.join(clip.name), clips.join(format!("{}.recording", clip.name)));
    remove(&recording);
    std::fs::create_dir_all(recording.join("frames")).unwrap();
    let mut reel = Reel { dir: recording.clone(), name: clip.name, video: None, last: None, frames: 0 };
    let done = record(clip, &text_system, &mut |frame, pointer| reel.add(frame, pointer));
    let frames = reel.frames;
    reel.video.expect("a clip has a frame").finish();
    std::fs::write(recording.join("shots.txt"), shots(&done, frames)).unwrap();
    remove(&dir);
    std::fs::rename(&recording, &dir).unwrap();
    eprintln!("wrote {}", dir.canonicalize().unwrap().display());
}

/// Where a clip's frames go as they're drawn.
struct Reel {
    dir: PathBuf,
    name: &'static str,
    /// Started with the first frame, at its size.
    video: Option<Video>,
    /// The frame before, as it was drawn.
    last: Option<RgbaImage>,
    frames: usize,
}

impl Reel {
    fn add(&mut self, frame: &RgbaImage, pointer: Option<Pointer>) {
        let path = self.dir.join(format!("{}.mp4", self.name));
        let video = self.video.get_or_insert_with(|| Video::start(&path, frame.width(), frame.height()));
        match pointer {
            Some(pointer) => {
                let mut frame = frame.clone();
                pointer::draw(&mut frame, pointer);
                video.add(&frame);
            }
            None => video.add(frame),
        }
        if self.last.as_ref() != Some(frame) {
            frame.save(self.dir.join("frames").join(format!("{:04}.png", self.frames))).unwrap();
            self.last = Some(frame.clone());
        }
        self.frames += 1;
    }
}

fn remove(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => panic!("can't remove {dir:?}: {error}"),
        Ok(()) | Err(_) => {}
    }
}

/// What was done in it, a line each, by the frame and the time it first shows at, and how long it is.
fn shots(done: &[Done], frames: usize) -> String {
    let at = |frame: usize| f64::from(u32::try_from(frame).unwrap()) / f64::from(FPS);
    let mut said = String::new();
    for Done { frame, said: what } in done {
        writeln!(said, "frame {frame:4}, {:6.3} s: {what}", at(*frame)).unwrap();
    }
    writeln!(said, "{frames} frames, {:.3} s", at(frames)).unwrap();
    said
}
