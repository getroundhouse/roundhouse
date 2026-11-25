//! A run: each set's clips heard under each condition, into a run's folder, stamped with what it heard with.

use std::path::Path;
use std::process::Command;

use anyhow::Context as _;
use dictation::nemotron::MODEL;
use serde::Deserialize;

use crate::cache::Cache;
use crate::clip::Set;
use crate::hear_all::{clip_audio, hear_all};
use crate::hearing::Hearer;
use crate::results::{Run, Stamp};
use crate::sets::{self, LIBRISPEECH, SoundAlike, TECH_SENTENCES};

/// The words said in the training split, and what sounds like them.
const SOUND_ALIKES: &str = include_str!("../sound-alikes.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SoundAlikes {
    pair: Vec<SoundAlike>,
}

/// How many of `set`'s clips a run hears unless it's told otherwise, or all of them: more of Tech-Sentences, which is
/// what the hints are for, than of `LibriSpeech`, which only has to show a change doesn't hurt everyday speech, and
/// every clip of the sound-alikes, which have only eight a word.
pub(crate) fn clips_heard(set: Set) -> Option<usize> {
    match set {
        Set::TechSentences => Some(300),
        Set::Librispeech => Some(100),
        Set::SoundAlikes | Set::Capitals => None,
    }
}

/// Hears, of each set, as many clips as `clips` says, or all of them, with the model's files in `model` and hints
/// boosted by `boost`, into the run in `out`.
pub(crate) fn run(
    out: &Path,
    clips: &[(Set, Option<usize>)],
    boost: Option<f32>,
    model: &Path,
    cache: &Cache,
) -> anyhow::Result<()> {
    let stamp = Stamp {
        model: MODEL.revision.to_string(),
        boost,
        datasets: [("tech-sentences", TECH_SENTENCES), ("librispeech", LIBRISPEECH)]
            .into_iter()
            .map(|(set, revision)| (set.to_string(), revision.to_string()))
            .collect(),
        clips: clips.iter().map(|&(set, count)| (set.name().to_string(), count)).collect(),
        code: code(out)?,
        machine: machine(),
    };
    let mut run = Run::open(out, stamp)?;
    let sound_alikes: SoundAlikes =
        toml::from_str(SOUND_ALIKES).context("sound-alikes.toml isn't what it should be")?;
    let model = model.to_path_buf();
    let hearer = Hearer::start(move || crate::model::engine(&model, boost))?;
    eprintln!("loading the model");
    hearer.prepare()?;
    for &(set, count) in clips {
        let clips = sets::clips(set, count, &sound_alikes.pair, cache)?;
        eprintln!("{}: {} clips", set.name(), clips.len());
        hear_all(&hearer, &clips, &|clip| clip_audio(cache, clip), &mut run)?;
    }
    eprintln!("done: speech-eval compare {}", out.display());
    Ok(())
}

/// The code a run is heard with: the commit, and the tree of every file in the checkout as it is, changes and new
/// files too, so a run resumed after a change refuses to mix the two. The tree is written with an index of its own in
/// `out`, so the checkout's index isn't touched.
pub(crate) fn code(out: &Path) -> anyhow::Result<String> {
    std::fs::create_dir_all(out).with_context(|| format!("couldn't make {}", out.display()))?;
    let index = out.join(".tree-index");
    let checkout_index = git(&["rev-parse", "--path-format=absolute", "--git-path", "index"], None)?;
    std::fs::copy(&checkout_index, &index).context("couldn't copy the checkout's index")?;
    // Leaving out the run's own folder, whose files change as it's heard, if it's in the checkout and git doesn't
    // ignore it already, since git refuses to be told to leave out what it ignores.
    let top = std::path::PathBuf::from(git(&["rev-parse", "--show-toplevel"], None)?);
    let inside = out.canonicalize()?.strip_prefix(top.canonicalize()?).map(Path::to_path_buf).ok();
    let leave_out =
        inside.filter(|inside| !ignored(&top.join(inside))).map(|inside| format!(":(top,exclude){}", inside.display()));
    let mut add = vec!["add", "-A", "--", ":/"];
    add.extend(leave_out.as_deref());
    let tree = git(&add, Some(&index)).and_then(|_| git(&["write-tree"], Some(&index)));
    let _ = std::fs::remove_file(&index);
    let commit = git(&["rev-parse", "--short=12", "HEAD"], None)?;
    Ok(format!("{commit}, tree {}", &tree?[..12]))
}

/// Whether git ignores `path`.
fn ignored(path: &Path) -> bool {
    Command::new("git").arg("check-ignore").arg("-q").arg(path).status().is_ok_and(|status| status.success())
}

/// What `git` writes, run with `index` as its index if it's given.
fn git(args: &[&str], index: Option<&Path>) -> anyhow::Result<String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output().context("couldn't run git")?;
    anyhow::ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// The Mac and its system, which say where the times are from.
pub(crate) fn machine() -> String {
    let output = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map_or_else(String::new, |text| text.trim().to_string())
    };
    format!(
        "{}, macOS {}",
        output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        output("sw_vers", &["-productVersion"])
    )
}
