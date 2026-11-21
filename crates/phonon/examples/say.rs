//! Says a sentence with the model through the speakers, and how fast it was: to check the engine and the speakers by
//! hand, since the model is too big for CI, and CI has no speakers.
//!
//! `cargo run -p phonon --release --example say -- <model directory> "<sentence>"`, where the directory holds the
//! files `phonon::MODEL` lists, or is where they're downloaded to first, as the app keeps them, by revision.

#![expect(clippy::unwrap_used, reason = "a tool run by hand, which stops at the first problem")]

use std::ops::ControlFlow;
use std::path::Path;
use std::time::Instant;

use phonon::{MODEL, Phonon};
use sound::{Played, Speakers};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model_dir, sentence] = &args[..] else { panic!("a model directory and a sentence") };
    let model_dir = Path::new(model_dir);
    if !MODEL.is_here(model_dir) {
        eprintln!("downloading {} MB to {}", MODEL.size_bytes() / 1_000_000, model_dir.display());
        MODEL.download(model_dir, &MODEL.hugging_face(), &mut |_| ControlFlow::Continue(())).unwrap();
    }
    let loading = Instant::now();
    let phonon = Phonon::load(&MODEL.dir(model_dir)).unwrap();
    eprintln!("loaded in {:.2}s", loading.elapsed().as_secs_f64());

    let mut speakers = Speakers::open(phonon.sample_rate()).unwrap();
    let speaking = Instant::now();
    let (mut first, mut made) = (None, 0);
    for audio in phonon.speak(sentence).unwrap() {
        let audio = audio.unwrap();
        first.get_or_insert_with(|| speaking.elapsed());
        made += audio.len();
        speakers.play(&audio);
    }
    let making = speaking.elapsed();
    speakers.mark(1);
    if let Played::Failed(e) = speakers.played().recv().unwrap() {
        panic!("{e}");
    }
    #[expect(clippy::cast_precision_loss, reason = "seconds of audio")]
    let lasts = made as f64 / f64::from(phonon.sample_rate());
    eprintln!(
        "first audio in {:.0} ms, {lasts:.2}s of audio made in {:.2}s ({:.1}x real time), played by {:.2}s",
        first.unwrap_or(making).as_secs_f64() * 1000.0,
        making.as_secs_f64(),
        lasts / making.as_secs_f64(),
        speaking.elapsed().as_secs_f64(),
    );
}
