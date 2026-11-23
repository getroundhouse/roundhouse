//! Hearing recordings given by hand, to look into one: through dictation, as a run hears its clips, or with the
//! engine alone, live and then whole.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use dictation::{Hints, Language};

use crate::audio::decode;
use crate::hearing::Hearer;

/// How much audio the engine takes at a time, as the microphone gives it: 10 ms.
const BUFFER: usize = dictation::Samples::from_millis(10).0;

/// Hears each of `files` biased toward `hints`, in `language` or the model's choice, and prints a JSON line for each.
pub(crate) fn hear(
    files: &[PathBuf],
    hints: Vec<String>,
    language: Option<&str>,
    engine_only: bool,
    model: &Path,
) -> anyhow::Result<()> {
    let language = match language {
        Some(code) => Language::from_code(code).with_context(|| format!("no language has the code {code:?}"))?,
        None => Language::default(),
    };
    let hints = Hints::new(hints);
    let read = |file: &PathBuf| {
        std::fs::read(file)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| decode(&bytes))
            .with_context(|| format!("couldn't read {}", file.display()))
    };
    if engine_only {
        let engine = crate::model::engine(model, None)?;
        for file in files {
            let audio = read(file)?;
            ensure!(audio.len() <= engine.longest_piece().0, "{} is longer than the 15 s heard whole", file.display());
            let mut listening = engine.listen(&hints, language);
            let mut live = String::new();
            for buffer in audio.chunks(BUFFER) {
                live.extend(listening.hear(buffer)?.into_iter().map(|heard| heard.text));
            }
            live.extend(listening.finish()?.into_iter().map(|heard| heard.text));
            let whole = engine.transcribe(&audio, &hints, language)?;
            println!("{}", serde_json::json!({ "file": file, "live": live.trim(), "final": whole }));
        }
        return Ok(());
    }
    let model = model.to_path_buf();
    let hearer = Hearer::start(move || crate::model::engine(&model, None))?;
    hearer.prepare()?;
    for file in files {
        let written = hearer.hear(&read(file)?, hints.clone(), language)?;
        println!("{}", serde_json::json!({ "file": file, "live": written.live, "final": written.final_text }));
    }
    Ok(())
}
