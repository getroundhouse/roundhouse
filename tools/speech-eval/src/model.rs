//! The model's files, and the engine loaded from them.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use dictation::Engine;
use dictation::nemotron::{MODEL, Nemotron};

use crate::cache::Cache;

/// The folder the model's files are in: `given`, once it holds the pinned revision's files, each as long as it
/// should be, so a run is stamped with the model it heard with, or else the cache's, downloaded first if they aren't
/// there.
pub(crate) fn files(given: Option<PathBuf>, cache: &Cache) -> anyhow::Result<PathBuf> {
    ensure!(
        dictation::nemotron::runs_here(),
        "dictation's model runs only on a Mac with macOS {} or later",
        dictation::nemotron::OLDEST_MACOS
    );
    match given {
        Some(dir) => pinned(&dir).map(|()| dir),
        None => download(cache),
    }
}

/// The engine from the files in `dir`, boosting hints by `boost` rather than the app's boost, if it's given.
pub(crate) fn engine(dir: &Path, boost: Option<f32>) -> Result<Box<dyn Engine>, dictation::EngineError> {
    // Each clip is heard under every condition in turn, so its audio is encoded once.
    let engine = Nemotron::load(dir)?.remembering_encodings();
    Ok(Box::new(match boost {
        Some(boost) => engine.with_hint_boost(boost),
        None => engine,
    }))
}

fn pinned(dir: &Path) -> anyhow::Result<()> {
    for file in MODEL.files {
        let at = dir.join(file.path);
        let length = std::fs::metadata(&at).map(|metadata| metadata.len());
        ensure!(
            length.ok() == Some(file.size_bytes),
            "{} isn't the model's revision {}: {} isn't there, or isn't {} bytes",
            dir.display(),
            MODEL.revision,
            at.display(),
            file.size_bytes
        );
    }
    Ok(())
}

fn download(cache: &Cache) -> anyhow::Result<PathBuf> {
    let dir = cache.dir().join("nemotron");
    if !MODEL.is_here(&dir) {
        eprintln!("downloading the model ({} GB)", MODEL.size_bytes() / 1_000_000_000);
        let mut shown = 0;
        let mut progress = |progress: models::Progress| {
            let percent = progress.done_bytes * 100 / progress.total_bytes.max(1);
            if percent >= shown + 10 {
                shown = percent;
                eprintln!("{percent}%");
            }
            ControlFlow::Continue(())
        };
        MODEL.download(&dir, &MODEL.hugging_face(), &mut progress).context("couldn't download the model")?;
    }
    Ok(MODEL.dir(&dir))
}
