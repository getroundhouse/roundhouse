//! Hears WAV files with the model, live and whole, and prints what it heard as a JSON line for each: to check the
//! engine against the model's files by hand, on a Mac, since they're too big for CI. Each file is 16-bit mono PCM at
//! 16 kHz, and no longer than 15 s.
//!
//! `cargo run -p dictation --release --example transcribe -- <model directory> <file.wav>...`, where the directory
//! holds the files `dictation::nemotron::MODEL` lists. The first run compiles the encoders for the Neural Engine,
//! which takes tens of seconds.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a tool run by hand, which stops at the first problem")]

use std::path::Path;
use std::time::Instant;

use dictation::nemotron::Nemotron;
use dictation::{Engine as _, Hints};

fn main() {
    let mut hint_words = Vec::new();
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--hint" {
            hint_words.push(args.next().expect("a word after --hint"));
        } else {
            positional.push(arg);
        }
    }
    let (directory, files) = positional.split_first().expect("a model directory and WAV files");
    let hints = Hints::new(hint_words);
    let started = Instant::now();
    let engine = Nemotron::load(Path::new(directory)).unwrap();
    eprintln!("loaded in {:?}", started.elapsed());
    for file in files {
        let audio = read(Path::new(file));
        let mut listening = engine.listen(&hints);
        let mut live = String::new();
        // 10 ms at a time, as the microphone gives it.
        for buffer in audio.chunks(160) {
            live.extend(listening.hear(buffer).unwrap().into_iter().map(|heard| heard.text));
        }
        live.extend(listening.finish().unwrap().into_iter().map(|heard| heard.text));
        let whole = engine.transcribe(&audio, &hints).unwrap();
        println!("{}", serde_json::json!({ "file": file, "live": live.trim(), "whole": whole }));
    }
}

/// The samples of a 16-bit mono WAV file at 16 kHz.
fn read(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF", "{} isn't a WAV file", path.display());
    let mut at = 12;
    let mut format = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        match id {
            b"fmt " => {
                let channels = u16::from_le_bytes([body[2], body[3]]);
                format = Some((channels, u32::from_le_bytes(body[4..8].try_into().unwrap())));
            }
            b"data" => {
                assert_eq!(format, Some((1, 16_000)), "{} isn't mono at 16 kHz", path.display());
                return body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
                    .collect();
            }
            _ => {}
        }
        at += 8 + size + size % 2;
    }
    panic!("{} has no audio", path.display())
}
