//! Gets this device's voice ready, fetching Phonon's files into a folder if they aren't there, then reads a text out
//! through the join `client-core`'s reading out uses, printing each update with its time: to check the voice by hand,
//! since CI has neither the files nor speakers.
//!
//! `cargo run -p conversation --release --example voice -- <model directory> "<text>"`

use std::time::{Duration, Instant};

use client_core::conversation::{FromVoice, ToVoice, Voice as _, VoiceReady, VoiceText};
use conversation::Voices;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model_dir, text] = &args[..] else { panic!("a model directory and a text") };
    let started = Instant::now();
    let at = || started.elapsed().as_secs_f64();
    let voices = Voices::new(model_dir.into(), "this device");
    voices.get_ready();
    loop {
        match voices.ready() {
            VoiceReady::Ready => break,
            VoiceReady::Failed(why) => panic!("couldn't get the voice ready: {why}"),
            VoiceReady::Fetching(done) => println!("{:6.2}s fetching {done:?}", at()),
        }
        let _ = voices.changed().recv_timeout(Duration::from_secs(5));
    }
    println!("{:6.2}s ready", at());
    let mut voice = voices.voice();
    let said = voice.said();
    voice.act(ToVoice::Say { text: VoiceText(1), at: 0, more: text.clone() });
    voice.act(ToVoice::End(VoiceText(1)));
    while let Ok(update) = said.recv() {
        let read = voice.read(update);
        println!("{:6.2}s {read:?}", at());
        if matches!(read, Some(FromVoice::Finished(_) | FromVoice::Stopped { .. })) {
            return;
        }
    }
}
