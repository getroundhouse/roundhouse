//! What a run writes, outside the repository, and the baseline checked in beside the eval.
//!
//! A run's folder has `run.json`, what it was run with and on, and `heard.jsonl`, a line for each clip under each
//! condition as it's heard: what was said and written, live and final, each with its counts and how it differs. A run
//! stopped partway goes on from where it got to. The baseline keeps only the counts, by clip, so none of what's said
//! in a dataset goes in the repository.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};

use crate::clip::{Clip, Condition, Set, Source};
use crate::hearing::Written;
use crate::lines;
use crate::score::{Counts, diff, score};

/// What a run was run with, and on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stamp {
    /// The model's revision.
    pub(crate) model: String,
    /// The hint boost, unless it's the app's.
    pub(crate) boost: Option<f32>,
    /// Each set's revision.
    pub(crate) datasets: BTreeMap<String, String>,
    /// The sets heard, by name, and how many of each one's clips, unless it was all of them.
    pub(crate) clips: BTreeMap<String, Option<usize>>,
    /// The commit, and the tree of every file in the checkout as it was, which a run that's gone on with has to have
    /// too, so a change to the code partway isn't mixed into it.
    pub(crate) code: String,
    /// The Mac and its system, which say where the times are from.
    pub(crate) machine: String,
}

impl Stamp {
    /// Whether `other` heard clips the same way, so its texts are comparable, whatever code it was heard with and
    /// wherever it ran.
    pub(crate) fn hears_as(&self, other: &Self) -> bool {
        let settings = |stamp: &Self| Self { code: String::new(), machine: String::new(), ..stamp.clone() };
        settings(self) == settings(other)
    }
}

/// One clip heard under one condition.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hearing {
    /// The clip's set, then its name.
    pub(crate) clip: String,
    pub(crate) condition: Condition,
    /// How many hint words the engine was given.
    pub(crate) hints: usize,
    pub(crate) said: String,
    /// The clip's terms and the words hinted that it doesn't say, so it can be scored again as it's read.
    pub(crate) terms: Vec<String>,
    pub(crate) sound_alikes: Vec<String>,
    pub(crate) unrelated: Vec<String>,
    pub(crate) live: Text,
    #[serde(rename = "final")]
    pub(crate) final_text: Text,
    /// How long the clip is.
    pub(crate) audio_ms: u64,
    /// How long the engine took over it, and from its end to its last final text. Only a clip's first hearing's
    /// are what a person would wait, since the others reuse its encodings.
    pub(crate) engine_ms: u64,
    pub(crate) after_ms: u64,
}

impl Hearing {
    /// `clip` heard under `condition` with `hints` hint words, as `written`, scored.
    pub(crate) fn new(clip: &Clip, condition: Condition, hints: usize, written: &Written, audio_ms: u64) -> Self {
        Self {
            clip: clip.key(),
            condition,
            hints,
            said: clip.text.clone(),
            terms: clip.terms.clone(),
            sound_alikes: clip.sound_alikes.clone(),
            unrelated: clip.unrelated.clone(),
            live: Text::of(clip, &written.live),
            final_text: Text::of(clip, &written.final_text),
            audio_ms,
            engine_ms: written.engine_ms,
            after_ms: written.after_ms,
        }
    }

    /// Scored again, as the scorer scores now, so a change to the scorer needs no run of the model.
    fn rescored(self) -> Self {
        let Some(set) = self.clip.split_once('/').and_then(|(set, _)| Set::named(set)) else { return self };
        let clip = Clip {
            set,
            id: String::new(),
            text: self.said.clone(),
            terms: self.terms.clone(),
            sound_alikes: self.sound_alikes.clone(),
            unrelated: self.unrelated.clone(),
            audio: Source::Samples(Vec::new()),
        };
        Self {
            live: Text::of(&clip, &self.live.written),
            final_text: Text::of(&clip, &self.final_text.written),
            ..self
        }
    }
}

impl Text {
    fn of(clip: &Clip, written: &str) -> Self {
        Self { written: written.to_string(), counts: score(clip, written), diff: diff(&clip.text, written) }
    }
}

/// One text written of a clip.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Text {
    pub(crate) written: String,
    pub(crate) counts: Counts,
    /// How it differs from what was said, word by word.
    pub(crate) diff: String,
}

/// A run's folder.
pub(crate) struct Run {
    dir: PathBuf,
    pub(crate) stamp: Stamp,
    pub(crate) heard: Vec<Hearing>,
}

impl Run {
    /// The run in `dir`, which is started if it's empty, and gone on with if it was run with `stamp` before.
    pub(crate) fn open(dir: &Path, stamp: Stamp) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("couldn't make {}", dir.display()))?;
        if dir.join("run.json").exists() {
            let run = Self::read(dir)?;
            ensure!(
                run.stamp.hears_as(&stamp),
                "{} is a run with other settings, so give this run another folder",
                dir.display()
            );
            ensure!(
                run.stamp.code == stamp.code,
                "{} was heard with other code, {}, against {} now, so give this run another folder",
                dir.display(),
                run.stamp.code,
                stamp.code
            );
            lines::cut_to_last_whole(dir)?;
            return Ok(run);
        }
        let json = serde_json::to_string_pretty(&stamp)?;
        std::fs::write(dir.join("run.json"), json + "\n").context("couldn't write run.json")?;
        Ok(Self { dir: dir.to_path_buf(), stamp, heard: Vec::new() })
    }

    /// The run in `dir`, as far as it got.
    pub(crate) fn read(dir: &Path) -> anyhow::Result<Self> {
        let stamp = read_json(&dir.join("run.json"))?;
        let heard = lines::read::<Hearing>(dir)?.into_iter().map(Hearing::rescored).collect();
        Ok(Self { dir: dir.to_path_buf(), stamp, heard })
    }

    pub(crate) fn has(&self, clip: &str, condition: Condition) -> bool {
        self.heard.iter().any(|heard| heard.clip == clip && heard.condition == condition)
    }

    /// Adds `heard`, written at once.
    pub(crate) fn add(&mut self, heard: Hearing) -> anyhow::Result<()> {
        lines::add(&self.dir, &heard)?;
        self.heard.push(heard);
        Ok(())
    }

    /// Its counts, as a baseline keeps them.
    pub(crate) fn baseline(&self) -> Baseline {
        let mut clips: BTreeMap<String, BTreeMap<Condition, Both>> = BTreeMap::new();
        for heard in &self.heard {
            let both = Both { live: heard.live.counts, final_text: heard.final_text.counts };
            clips.entry(heard.clip.clone()).or_default().insert(heard.condition, both);
        }
        Baseline { run: self.stamp.clone(), scorer: crate::score::SCORER, times: self.times(), clips }
    }

    /// The times of each clip's first hearing, since the others reuse its encodings.
    fn times(&self) -> Times {
        let first: Vec<&Hearing> = self.heard.iter().filter(|heard| heard.condition == Condition::HEARD[0]).collect();
        let mut after: Vec<u64> = first.iter().map(|heard| heard.after_ms).collect();
        after.sort_unstable();
        let at = |share: usize| after.get((after.len() * share / 100).min(after.len().saturating_sub(1))).copied();
        let hearing: u64 = first.iter().map(|heard| heard.engine_ms).sum();
        let audio: u64 = first.iter().map(|heard| heard.audio_ms).sum();
        Times {
            median_after_ms: at(50).unwrap_or_default(),
            p90_after_ms: at(90).unwrap_or_default(),
            engine_ms_per_s: (hearing * 1000).checked_div(audio).unwrap_or_default(),
            machine: self.stamp.machine.clone(),
        }
    }
}

/// A run's counts by clip and condition, checked in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Baseline {
    pub(crate) run: Stamp,
    /// The version of the scorer its counts are from, which a run is scored by again as it's read, but a baseline,
    /// holding only counts, can't be.
    pub(crate) scorer: u32,
    pub(crate) times: Times,
    pub(crate) clips: BTreeMap<String, BTreeMap<Condition, Both>>,
}

/// How long each clip's first hearing took, which depends on the Mac, so it's only compared.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Times {
    /// From the end of a clip to its last final text.
    pub(crate) median_after_ms: u64,
    pub(crate) p90_after_ms: u64,
    /// How many milliseconds the engine took for each second of audio.
    pub(crate) engine_ms_per_s: u64,
    /// Where they're from.
    pub(crate) machine: String,
}

/// The live text's counts and the final text's.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Both {
    pub(crate) live: Counts,
    #[serde(rename = "final")]
    pub(crate) final_text: Counts,
}

impl Baseline {
    pub(crate) fn read(path: &Path) -> anyhow::Result<Self> {
        read_json(path)
    }

    /// Writes it with a line for each clip, so a change to a clip's counts is a line of the diff.
    pub(crate) fn write(&self, path: &Path) -> anyhow::Result<()> {
        let mut json = format!(
            "{{\n  \"run\": {},\n  \"scorer\": {},\n  \"times\": {},\n  \"clips\": {{\n",
            serde_json::to_string(&self.run)?,
            self.scorer,
            serde_json::to_string(&self.times)?
        );
        for (i, (clip, conditions)) in self.clips.iter().enumerate() {
            let comma = if i + 1 < self.clips.len() { "," } else { "" };
            let _ =
                writeln!(json, "    {}: {}{comma}", serde_json::to_string(clip)?, serde_json::to_string(conditions)?);
        }
        json.push_str("  }\n}\n");
        std::fs::write(path, json).with_context(|| format!("couldn't write {}", path.display()))
    }

    /// The counts added up by set and condition.
    pub(crate) fn totals(&self) -> BTreeMap<(String, Condition), Both> {
        let mut totals: BTreeMap<(String, Condition), Both> = BTreeMap::new();
        for (clip, conditions) in &self.clips {
            let set = clip.split('/').next().unwrap_or_default().to_string();
            for (condition, both) in conditions {
                let total = totals.entry((set.clone(), *condition)).or_default();
                total.live.add(&both.live);
                total.final_text.add(&both.final_text);
            }
        }
        totals
    }
}

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(path).with_context(|| format!("couldn't read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} isn't what it should be", path.display()))
}
