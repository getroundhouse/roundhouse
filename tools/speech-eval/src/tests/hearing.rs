//! The eval run against `FakeEngine`, which stands in for the model, so the tool doesn't break between runs by hand.

use std::collections::BTreeMap;

use crate::clip::{Clip, Condition, Set, Source};
use crate::compare::compare;
use crate::hear_all::hear_all;
use crate::hearing::Hearer;
use crate::results::{Baseline, Run, Stamp};
use dictation::Engine;
use dictation::fake::FakeEngine;

const SCRIPT: &[&str] = &["fix", "the", "test", "run", "it"];

fn stamp() -> Stamp {
    Stamp {
        model: "fake".to_string(),
        boost: None,
        datasets: BTreeMap::new(),
        clips: BTreeMap::from([("tech-sentences".to_string(), None)]),
        code: "a commit".to_string(),
        machine: "a test".to_string(),
    }
}

fn clip(engine: &FakeEngine) -> Clip {
    Clip {
        set: Set::TechSentences,
        id: "fix-it".to_string(),
        text: "Fix the test. Run it.".to_string(),
        terms: vec!["test".to_string()],
        sound_alikes: Vec::new(),
        unrelated: vec!["Grafana".to_string(), "Typst".to_string()],
        audio: Source::Samples(engine.saying("fix the test. / run it.")),
    }
}

fn hearer(engine: &FakeEngine) -> Hearer {
    let engine = engine.clone();
    Hearer::start(move || Ok(Box::new(engine.clone()) as Box<dyn Engine>)).unwrap()
}

fn given(clip: &Clip) -> anyhow::Result<Vec<f32>> {
    match &clip.audio {
        Source::Samples(samples) => Ok(samples.clone()),
        Source::Range { .. } | Source::Cached(_) => anyhow::bail!("only given samples"),
    }
}

#[test]
fn each_clip_is_heard_under_each_condition_and_scored() {
    let engine = FakeEngine::new(SCRIPT);
    let dir = tempfile::tempdir().unwrap();
    let mut run = Run::open(dir.path(), stamp()).unwrap();

    hear_all(&hearer(&engine), &[clip(&engine)], &given, &mut run).unwrap();

    let heard = Run::read(dir.path()).unwrap().heard;
    let conditions: Vec<Condition> = heard.iter().map(|heard| heard.condition).collect();
    assert_eq!(conditions, Condition::HEARD);
    assert_eq!(heard.iter().map(|heard| heard.hints).collect::<Vec<_>>(), [3, 1, 0]);
    for heard in &heard {
        assert_eq!(heard.final_text.written, "Fix the test. Run it.");
        assert_eq!(heard.final_text.diff, "fix the test run it");
        assert_eq!((heard.final_text.counts.terms, heard.final_text.counts.right), (1, 1));
        assert_eq!(heard.live.diff, "fix the test run it", "{}", heard.live.written);
        assert_eq!(heard.clip, "tech-sentences/fix-it");
    }
}

#[test]
fn a_run_goes_on_from_where_it_stopped() {
    let engine = FakeEngine::new(SCRIPT);
    let dir = tempfile::tempdir().unwrap();
    let clip = clip(&engine);
    let mut run = Run::open(dir.path(), stamp()).unwrap();
    hear_all(&hearer(&engine), std::slice::from_ref(&clip), &given, &mut run).unwrap();

    let mut again = Run::open(dir.path(), stamp()).unwrap();
    hear_all(&hearer(&engine), &[clip], &|_| anyhow::bail!("heard again"), &mut again).unwrap();

    assert_eq!(Run::read(dir.path()).unwrap().heard.len(), 3);
}

#[test]
fn a_run_with_other_settings_needs_a_folder_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    Run::open(dir.path(), stamp()).unwrap();

    let other = Run::open(dir.path(), Stamp { boost: Some(3.0), ..stamp() });

    assert!(other.is_err());
    assert!(Run::open(dir.path(), Stamp { code: "another commit".to_string(), ..stamp() }).is_err());
    assert!(Run::open(dir.path(), Stamp { machine: "another Mac".to_string(), ..stamp() }).is_ok());
}

#[test]
fn compare_shows_what_moved_from_the_baseline() {
    let engine = FakeEngine::new(SCRIPT);
    let dir = tempfile::tempdir().unwrap();
    let mut run = Run::open(dir.path(), stamp()).unwrap();
    hear_all(&hearer(&engine), &[clip(&engine)], &given, &mut run).unwrap();
    let run = Run::read(dir.path()).unwrap();
    let path = dir.path().join("baseline.json");
    run.baseline().write(&path).unwrap();
    let mut baseline = Baseline::read(&path).unwrap();

    assert_eq!(baseline, run.baseline(), "a baseline reads as it was written");
    assert!(compare(&run, &baseline).contains("No clip's terms right or unsaid hints written changed."));

    let then = baseline.clips.get_mut("tech-sentences/fix-it").unwrap().get_mut(&Condition::Said).unwrap();
    then.final_text.right = 0;
    let compared = compare(&run, &baseline);
    let (final_text, live) = compared.split_once("**Live text**").unwrap();
    assert!(final_text.contains("| tech-sentences | said | 1 of 1 (+1) |"), "{compared}");
    assert!(live.contains("| tech-sentences | said | 1 of 1 |"), "{compared}");
    assert!(compared.contains("- `tech-sentences/fix-it`, said: final 0 → 1 right"), "{compared}");

    let mut bigger = run.baseline();
    let another = bigger.clips["tech-sentences/fix-it"].clone();
    bigger.clips.insert("tech-sentences/not-heard".to_string(), another);
    let compared = compare(&run, &bigger);
    assert!(
        compared.contains("| tech-sentences | said | 1 of 1 | 0 |"),
        "only what both heard is added up: {compared}"
    );
    assert!(
        compared.contains("0 clips under a condition aren't in the baseline, and 3 in it weren't heard."),
        "{compared}"
    );
}

#[test]
fn a_line_cut_short_as_a_run_stopped_is_heard_again() {
    let engine = FakeEngine::new(SCRIPT);
    let dir = tempfile::tempdir().unwrap();
    let mut run = Run::open(dir.path(), stamp()).unwrap();
    hear_all(&hearer(&engine), &[clip(&engine)], &given, &mut run).unwrap();
    let path = dir.path().join("heard.jsonl");
    let whole = std::fs::read_to_string(&path).unwrap();
    // Its last line, without the newline that ends it, as a run stopped between the two would leave it.
    std::fs::write(&path, &whole[..whole.len() - 1]).unwrap();
    assert_eq!(Run::read(dir.path()).unwrap().heard.len(), 2);

    let mut again = Run::open(dir.path(), stamp()).unwrap();
    hear_all(&hearer(&engine), &[clip(&engine)], &given, &mut again).unwrap();

    let read = Run::read(dir.path()).unwrap();
    assert_eq!(read.heard.len(), 3);
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!((after.lines().count(), after.ends_with('\n')), (3, true), "it's heard again, onto a line of its own");

    std::fs::write(&path, format!("not a line\n{whole}")).unwrap();
    assert!(Run::read(dir.path()).is_err(), "only a last line can be cut short");
}
