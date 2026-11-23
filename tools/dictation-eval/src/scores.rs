//! Every case in a folder, scored, and how the scores differ from a baseline's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::case::{Case, Invalid};
use crate::score::Score;

/// Why the cases or a baseline couldn't be read or written.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{}: {source}", path.display())]
    Case { path: PathBuf, source: toml::de::Error },
    #[error("{}: {source}", path.display())]
    Invalid { path: PathBuf, source: Invalid },
    #[error("{}: only cases, as .toml files, go in the folder", path.display())]
    NotACase { path: PathBuf },
    #[error("{}: {source}", path.display())]
    Baseline { path: PathBuf, source: serde_json::Error },
}

/// Every case's score, by its path in the cases' folder without `.toml`, such as `long-chat/relay`, as a baseline
/// keeps them.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
#[serde(transparent)]
pub struct Scores(BTreeMap<String, Score>);

impl Scores {
    /// Scores every case in `dir` and the folders in it.
    pub fn load(dir: &Path) -> Result<Self, EvalError> {
        let mut scores = BTreeMap::new();
        score_into(dir, "", &mut scores)?;
        Ok(Self(scores))
    }

    pub fn read(path: &Path) -> Result<Self, EvalError> {
        let text =
            std::fs::read_to_string(path).map_err(|source| EvalError::Io { path: path.to_path_buf(), source })?;
        serde_json::from_str(&text).map_err(|source| EvalError::Baseline { path: path.to_path_buf(), source })
    }

    pub fn write(&self, path: &Path) -> Result<(), EvalError> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|source| EvalError::Baseline { path: path.to_path_buf(), source })?;
        text.push('\n');
        std::fs::write(path, text).map_err(|source| EvalError::Io { path: path.to_path_buf(), source })
    }

    /// What's changed since `baseline`, a line for each case that differs, saying which terms it gained, lost, took
    /// on or left out, and how its hint words changed. Empty when nothing has.
    pub fn changes_from(&self, baseline: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        for name in baseline.0.keys().filter(|name| !self.0.contains_key(*name)) {
            changes.push(format!("{name}: no longer a case"));
        }
        for (name, now) in &self.0 {
            match baseline.0.get(name) {
                None => changes.push(format!("{name}: a new case")),
                Some(then) if then != now => changes.push(format!("{name}: {}", now.change_from(then))),
                Some(_) => {}
            }
        }
        changes
    }

    /// The scores together: how many terms are hinted, and how many hint words go unused.
    pub fn summary(&self) -> String {
        let hinted: usize = self.0.values().map(|score| score.hinted.len()).sum();
        let terms: usize = self.0.values().map(|score| score.hinted.len() + score.missed.len()).sum();
        let hints: usize = self.0.values().map(|score| score.hints).sum();
        let unused: usize = self.0.values().map(|score| score.unused).sum();
        format!(
            "{hinted} of {terms} terms hinted, in {} cases, and {unused} of {hints} hint words unused",
            self.0.len()
        )
    }
}

impl Score {
    /// How this score differs from `then`, the same case's: each term it now hints or no longer does, each it took on
    /// or left out, then its hint words.
    fn change_from(&self, then: &Self) -> String {
        let terms = |score: &Self| score.hinted.union(&score.missed).cloned().collect::<BTreeSet<_>>();
        let (now_terms, then_terms) = (terms(self), terms(then));
        let mut parts = Vec::new();
        for term in now_terms.union(&then_terms) {
            let part = match (self.hinted.contains(term), then.hinted.contains(term)) {
                _ if !now_terms.contains(term) => format!("no longer has the term {term:?}"),
                (true, _) if !then_terms.contains(term) => format!("a new term, {term:?}, hinted"),
                (false, _) if !then_terms.contains(term) => format!("a new term, {term:?}, missed"),
                (true, false) => format!("now hints {term:?}"),
                (false, true) => format!("no longer hints {term:?}"),
                (true, true) | (false, false) => continue,
            };
            parts.push(part);
        }
        parts.push(format!(
            "{} hint words, {} unused, from {} and {}",
            self.hints, self.unused, then.hints, then.unused
        ));
        parts.join(", ")
    }
}

fn score_into(dir: &Path, prefix: &str, scores: &mut BTreeMap<String, Score>) -> Result<(), EvalError> {
    let io = |source| EvalError::Io { path: dir.to_path_buf(), source };
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        let file_name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if path.is_dir() {
            score_into(&path, &format!("{prefix}{file_name}/"), scores)?;
            continue;
        }
        let Some(stem) = file_name.strip_suffix(".toml") else { return Err(EvalError::NotACase { path }) };
        let text = std::fs::read_to_string(&path).map_err(|source| EvalError::Io { path: path.clone(), source })?;
        let case: Case = toml::from_str(&text).map_err(|source| EvalError::Case { path: path.clone(), source })?;
        let score = case.score().map_err(|source| EvalError::Invalid { path: path.clone(), source })?;
        scores.insert(format!("{prefix}{stem}"), score);
    }
    Ok(())
}
