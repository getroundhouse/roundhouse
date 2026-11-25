//! A folder of recordings, as the app keeps them.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use kept_recordings::{AUDIO, FILE, How, Kept, Text};

/// One recording, and the folder it's in.
pub(super) struct Recording {
    pub(super) kept: Kept,
    dir: PathBuf,
}

impl Recording {
    /// Its audio, at the model's rate.
    pub(super) fn audio(&self) -> anyhow::Result<Vec<f32>> {
        let path = self.dir.join(AUDIO);
        let bytes = std::fs::read(&path).with_context(|| format!("couldn't read {}", path.display()))?;
        crate::audio::decode(&bytes).with_context(|| format!("couldn't hear {}", path.display()))
    }

    /// All that the app wrote of it, final, if it was heard to its end. One that was cancelled, that failed, or whose
    /// field or app went has less than was said.
    pub(super) fn written(&self) -> Option<String> {
        let whole = self.kept.ended.as_ref().is_some_and(|ended| match ended.how {
            How::Heard | How::NothingHeard => true,
            How::Cancelled | How::FieldGone | How::Failed => false,
        });
        whole.then(|| finals(&self.kept))
    }
}

/// The final texts of `kept`, one after another.
fn finals(kept: &Kept) -> String {
    let finals: Vec<&str> = kept
        .said
        .iter()
        .filter_map(|said| match &said.text {
            Text::Final(text) => Some(text.as_str()),
            Text::Live(_) => None,
        })
        .collect();
    finals.join(" ")
}

/// Every recording in `folder`, in the order they were dictated. What else is in it, such as what was sent, isn't one.
pub(super) fn read(folder: &Path) -> anyhow::Result<Vec<Recording>> {
    let entries = std::fs::read_dir(folder).with_context(|| format!("couldn't read {}", folder.display()))?;
    let mut recordings = Vec::new();
    for entry in entries {
        let dir = entry?.path();
        let file = dir.join(FILE);
        if file.exists() {
            let kept = crate::results::read_json(&file)?;
            recordings.push(Recording { kept, dir });
        }
    }
    recordings.sort_by(|a, b| (a.kept.started_at_ms, &a.kept.id).cmp(&(b.kept.started_at_ms, &b.kept.id)));
    Ok(recordings)
}
