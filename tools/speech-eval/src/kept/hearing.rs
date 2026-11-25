//! Hearing each recording again, and noting what was written as it goes.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use dictation::nemotron::MODEL;
use dictation::{Hints, Language, SAMPLE_RATE};
use kept_recordings::Kept;
use serde::{Deserialize, Serialize};

use super::recordings::{self, Recording};
use super::words::{Chats, ToHear};
use crate::hearing::{Hearer, Written};
use crate::{lines, running};

/// What recordings are heard with, in place of what each was.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) boost: Option<f32>,
    pub(crate) no_hints: bool,
    /// The words today's picker gives each, rather than those it was heard with.
    #[serde(default)]
    pub(crate) todays_words: bool,
}

/// What a run was run with, and on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Stamp {
    pub(super) settings: Settings,
    /// The model's revision.
    pub(super) model: String,
    /// The commit, and the tree of every file in the checkout as it was.
    pub(super) code: String,
    /// The Mac and its system.
    pub(super) machine: String,
}

impl Stamp {
    /// Whether a run with `other` can go on from one with this, whichever Mac each is on.
    fn goes_on(&self, other: &Self) -> bool {
        let wherever = |stamp: &Self| Self { machine: String::new(), ..stamp.clone() };
        wherever(self) == wherever(other)
    }
}

/// One recording, heard again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Heard {
    pub(super) id: String,
    pub(super) started_at_ms: u64,
    pub(super) audio_ms: u64,
    /// The revision of the model the app heard it with.
    pub(super) app_model: String,
    /// All that the app wrote of it, if it heard it to its end.
    pub(super) app_final: Option<String>,
    pub(super) live: String,
    #[serde(rename = "final")]
    pub(super) final_text: String,
}

/// A run's folder.
pub(super) struct Run {
    dir: PathBuf,
    pub(super) stamp: Stamp,
    pub(super) heard: Vec<Heard>,
}

impl Run {
    /// The run in `dir`, which is started if it's empty, and gone on with if it was run with `stamp` before.
    pub(super) fn open(dir: &Path, stamp: Stamp) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("couldn't make {}", dir.display()))?;
        let file = dir.join("run.json");
        if file.exists() {
            let run = Self::read(dir)?;
            ensure!(
                run.stamp.goes_on(&stamp),
                "{} is a run with other settings or code, so give this run another folder",
                dir.display()
            );
            lines::cut_to_last_whole(dir)?;
            return Ok(run);
        }
        std::fs::write(&file, serde_json::to_string_pretty(&stamp)? + "\n").context("couldn't write run.json")?;
        Ok(Self { dir: dir.to_path_buf(), stamp, heard: Vec::new() })
    }

    /// The run in `dir`, as far as it got.
    pub(super) fn read(dir: &Path) -> anyhow::Result<Self> {
        let stamp = crate::results::read_json(&dir.join("run.json"))?;
        Ok(Self { dir: dir.to_path_buf(), stamp, heard: lines::read(dir)? })
    }

    fn add(&mut self, heard: Heard) -> anyhow::Result<()> {
        lines::add(&self.dir, &heard)?;
        self.heard.push(heard);
        Ok(())
    }
}

/// Hears each recording in `recordings` with `settings` and the model's files in `model`, into the run in `out`. Today's
/// words are picked from the chats in the hub's database at `hub`.
pub(crate) fn hear(
    recordings: &Path,
    out: &Path,
    settings: Settings,
    hub: Option<&Path>,
    model: &Path,
) -> anyhow::Result<()> {
    let code = running::code(out)?;
    let stamp = Stamp { settings, model: MODEL.revision.to_string(), code, machine: running::machine() };
    let mut run = Run::open(out, stamp)?;
    let chats = hub.map(|hub| Chats::open(hub, out)).transpose()?;
    let ToHear { each, same, gone } = ToHear::of(recordings::read(recordings)?, settings, chats.as_ref())?;
    drop(chats);
    if hub.is_some() {
        eprintln!(
            "{same} of {} have the words they were heard with, and {gone} aren't heard, since what theirs were picked from \
             is gone or wasn't kept",
            each.len()
        );
    }
    let model = model.to_path_buf();
    hear_all(&each, &mut run, |boost| {
        let model = model.clone();
        let hearer = Hearer::start(move || crate::model::engine(&model, Some(boost)))?;
        eprintln!("loading the model, with a hint boost of {boost}");
        hearer.prepare()?;
        Ok(hearer)
    })?;
    eprintln!("done: speech-eval kept compare {}", out.display());
    Ok(())
}

/// Hears each of `to_hear` the run hasn't, with the hearer `start` gives for the boost it's heard with.
pub(super) fn hear_all(
    to_hear: &[(Recording, Hints)],
    run: &mut Run,
    mut start: impl FnMut(f32) -> anyhow::Result<Hearer>,
) -> anyhow::Result<()> {
    let settings = run.stamp.settings;
    let boost = |kept: &Kept| settings.boost.unwrap_or(kept.hint_boost);
    let already: Vec<String> = run.heard.iter().map(|heard| heard.id.clone()).collect();
    let mut left: Vec<&(Recording, Hints)> = to_hear.iter().filter(|(r, _)| !already.contains(&r.kept.id)).collect();
    // The boost is the engine's, and loading one takes a while, so those heard with the same one are heard together.
    left.sort_by(|(a, _), (b, _)| boost(&a.kept).total_cmp(&boost(&b.kept)));
    let mut hearing: Option<(f32, Hearer)> = None;
    for (i, (recording, hints)) in left.iter().enumerate() {
        let boost = boost(&recording.kept);
        let hearer = match &mut hearing {
            Some((with, hearer)) if with.total_cmp(&boost).is_eq() => hearer,
            other => &mut other.insert((boost, start(boost)?)).1,
        };
        let id = &recording.kept.id;
        let heard = hear_one(hearer, recording, hints).with_context(|| format!("couldn't hear {id} again"))?;
        run.add(heard)?;
        eprintln!("{} of {}: {id}", i + 1, left.len());
    }
    Ok(())
}

fn hear_one(hearer: &Hearer, recording: &Recording, hints: &Hints) -> anyhow::Result<Heard> {
    let kept = &recording.kept;
    let language =
        Language::from_code(&kept.language).with_context(|| format!("no language has the code {:?}", kept.language))?;
    let audio = recording.audio()?;
    let Written { live, final_text, .. } = hearer.hear(&audio, hints.clone(), language)?;
    Ok(Heard {
        id: kept.id.clone(),
        started_at_ms: kept.started_at_ms,
        audio_ms: u64::try_from(audio.len())? * 1000 / u64::from(SAMPLE_RATE),
        app_model: kept.model.clone(),
        app_final: recording.written(),
        live,
        final_text,
    })
}
