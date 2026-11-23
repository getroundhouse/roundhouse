//! Hearing each clip under each condition, and noting what was written as it goes.

use anyhow::Context as _;
use dictation::{Hints, Language, SAMPLE_RATE};

use crate::clip::{Clip, Condition, Source};
use crate::hearing::Hearer;
use crate::results::{Hearing, Run};

/// Hears each of `clips` under each condition `run` hasn't heard it under yet, with `audio` giving its samples, and
/// adds each to `run` as it's heard.
pub(crate) fn hear_all(
    hearer: &Hearer,
    clips: &[Clip],
    audio: &dyn Fn(&Clip) -> anyhow::Result<Vec<f32>>,
    run: &mut Run,
) -> anyhow::Result<()> {
    for (i, clip) in clips.iter().enumerate() {
        let key = clip.key();
        let conditions: Vec<Condition> = Condition::HEARD.into_iter().filter(|&c| !run.has(&key, c)).collect();
        if conditions.is_empty() {
            continue;
        }
        let samples = audio(clip).with_context(|| format!("couldn't read {key}'s audio"))?;
        for condition in conditions {
            let hints = Hints::new(clip.hint_words(condition));
            let count = hints.words().len();
            // The app's language unless a person chooses one, which leaves the model to tell.
            let written =
                hearer.hear(&samples, hints, Language::default()).with_context(|| format!("couldn't hear {key}"))?;
            let audio_ms = u64::try_from(samples.len())? * 1000 / u64::from(SAMPLE_RATE);
            run.add(Hearing::new(clip, condition, count, &written, audio_ms))?;
        }
        eprintln!("{} of {}: {key}", i + 1, clips.len());
    }
    Ok(())
}

/// A clip's samples, from wherever it is: fetched once and kept in the cache, read from it, or given.
pub(crate) fn clip_audio(cache: &crate::cache::Cache, clip: &Clip) -> anyhow::Result<Vec<f32>> {
    let bytes = match &clip.audio {
        Source::Range { url, start, length } => cache.range(url, *start, *length, &format!("clips/{}", clip.key()))?,
        Source::Cached(path) => {
            let at = cache.dir().join(path);
            std::fs::read(&at).with_context(|| format!("couldn't read {}", at.display()))?
        }
        Source::Samples(samples) => return Ok(samples.clone()),
    };
    crate::audio::decode(&bytes)
}
