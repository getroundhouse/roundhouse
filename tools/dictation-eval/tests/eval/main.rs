#![cfg(test)]

//! Every case's score against the baseline checked in beside the cases.

use std::path::Path;

use dictation_eval::Scores;

/// Set to accept every case's score now, as the baseline.
const UPDATE: &str = "ROUNDHOUSE_UPDATE_EVAL";

#[test]
fn every_case_scores_as_the_baseline_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scores = Scores::load(&root.join("cases")).unwrap();
    let path = root.join("baseline.json");

    if std::env::var_os(UPDATE).is_some() {
        scores.write(&path).unwrap();
        return;
    }
    assert!(path.exists(), "no baseline at {}. {UPDATE}=1 cargo test -p dictation-eval makes one", path.display());
    let baseline = Scores::read(&path).unwrap();
    let changes = scores.changes_from(&baseline);
    assert!(
        changes.is_empty(),
        "The cases' scores differ from the baseline's:\n\n{}\n\nNow: {}\nThen: {}\n\nIf the change is meant, \
         {UPDATE}=1 cargo test -p dictation-eval accepts it.",
        changes.join("\n"),
        scores.summary(),
        baseline.summary(),
    );
}
