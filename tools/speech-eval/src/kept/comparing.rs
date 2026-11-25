//! What a run wrote of each recording, set against what another run or the app did. No right text is known for most of
//! them, so those that came out differently are listed, to be judged by ear.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::hearing::{Heard, Run};
use crate::score::diff;

/// Lists what differs between the run in `run` and the one in `other`, or what the app wrote.
pub(crate) fn compare(run: &Path, other: Option<&Path>) -> anyhow::Result<String> {
    let run = Run::read(run)?;
    Ok(match other {
        Some(other) => between(&run, &Run::read(other)?),
        None => with_the_app(&run),
    })
}

/// How many recordings came out the same and not, and those that didn't.
#[derive(Default)]
struct Compared {
    same: usize,
    /// Those whose words are the same, but for capitals and marks.
    same_words: usize,
    other_words: usize,
    listed: String,
}

impl Compared {
    fn add(&mut self, heard: &Heard, then: &str) {
        if heard.final_text == then {
            self.same += 1;
            return;
        }
        let words = diff(then, &heard.final_text);
        if words.contains('[') {
            self.other_words += 1;
        } else {
            self.same_words += 1;
        }
        let Heard { id, audio_ms, final_text, .. } = heard;
        let seconds = audio_ms / 1000;
        let _ = writeln!(self.listed, "\n{id}, {seconds} s\n  was: {then}\n  now: {final_text}\n  {words}");
    }

    fn text(&self) -> String {
        let Self { same, same_words, other_words, listed } = self;
        format!(
            "{same} came out the same, {same_words} with other capitals or marks, and {other_words} with other \
             words.\n{listed}"
        )
    }
}

fn between(run: &Run, other: &Run) -> String {
    let then: BTreeMap<&str, &Heard> = other.heard.iter().map(|heard| (heard.id.as_str(), heard)).collect();
    let mut compared = Compared::default();
    let mut unheard = 0;
    for heard in in_order(run) {
        match then.get(heard.id.as_str()) {
            Some(then) => compared.add(heard, &then.final_text),
            None => unheard += 1,
        }
    }
    let mut out = compared.text();
    if unheard > 0 {
        let _ = writeln!(out, "\n{unheard} weren't heard in the other run.");
    }
    out
}

/// Set against what the app wrote as it was dictated, which the same model with the same settings writes again. One it
/// didn't hear to its end has less than was said, and one another model heard isn't expected to come out the same.
fn with_the_app(run: &Run) -> String {
    let mut compared = Compared::default();
    let (mut cut_short, mut other_model) = (0, 0);
    for heard in in_order(run) {
        match &heard.app_final {
            None => cut_short += 1,
            Some(_) if heard.app_model != run.stamp.model => other_model += 1,
            Some(written) => compared.add(heard, written),
        }
    }
    let mut out = compared.text();
    if cut_short > 0 {
        let _ = writeln!(out, "\n{cut_short} aren't set against the app's, which didn't hear them to their end.");
    }
    if other_model > 0 {
        let _ = writeln!(out, "\n{other_model} aren't set against the app's, which heard them with another model.");
    }
    out
}

/// What a run heard, in the order it was dictated.
fn in_order(run: &Run) -> Vec<&Heard> {
    let mut heard: Vec<&Heard> = run.heard.iter().collect();
    heard.sort_by_key(|heard| (heard.started_at_ms, &heard.id));
    heard
}
