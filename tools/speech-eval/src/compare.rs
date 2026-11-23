//! A run set against a baseline, in Markdown, to go on a pull request: each set's scores under each condition, final
//! and live, with how far each moved, then the clips whose terms or unsaid words changed, with what was written.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::clip::Condition;
use crate::results::{Baseline, Both, Run, Times};
use crate::score::{Counts, SCORER};

/// Which of a clip's texts a table is of.
type Pick = fn(&Both) -> &Counts;

/// Whose the recordings and what's said in them are, since what's written of them is quoted.
const ATTRIBUTION: &str = "\nThe recordings are [Tech-Sentences](https://huggingface.co/datasets/danielrosehill/Tech-Sentences), by \
    Daniel Rosehill, and [LibriSpeech](https://www.openslr.org/12)'s test-clean, both under CC BY 4.0, with Le et al.'s \
    [lists of words](https://github.com/facebookresearch/fbai-speech/tree/main/is21_deep_bias), under MIT.\n";

/// How many changed clips are listed, at most.
const LISTED: usize = 40;

pub(crate) fn compare(run: &Run, baseline: &Baseline) -> String {
    let now = run.baseline();
    let mut out = String::new();
    if !now.run.hears_as(&baseline.run) {
        let _ = writeln!(
            out,
            "This run heard clips otherwise than the baseline's: {:?}, against {:?}.\n",
            now.run, baseline.run
        );
    }
    if baseline.scorer != SCORER {
        let _ = writeln!(
            out,
            "The baseline's counts are from version {} of the scorer, and this run's from {SCORER}, so some of what \
             moved is the scorer's. Accept a run to set like against like.\n",
            baseline.scorer
        );
    }
    // Only what both heard, so a run of some clips isn't set against all of the baseline's.
    let (totals, then_totals) = (shared(&now, baseline).totals(), shared(baseline, &now).totals());
    let texts: [(&str, Pick); 2] = [("Final text", |both| &both.final_text), ("Live text", |both| &both.live)];
    for (title, text) in texts {
        let _ = writeln!(
            out,
            "**{title}**\n\n| Set | Hints | Terms right | Unsaid hints written | Other words' errors |\n|---|---|---|---|---|"
        );
        for ((set, condition), both) in &totals {
            let then = then_totals.get(&(set.clone(), *condition)).copied().unwrap_or_default();
            let (now, then) = (text(both), text(&then));
            let _ = writeln!(
                out,
                "| {set} | {} | {} of {}{} | {}{} | {} |",
                condition.name(),
                now.right,
                now.terms,
                moved(now.right, then.right),
                now.unsaid,
                moved(now.unsaid, then.unsaid),
                rate(now, then),
            );
        }
        out.push('\n');
    }
    out.push_str(
        "The errors are Le et al.'s U-WER: of the words said outside the terms, those written as another word or left \
         out, and the words put in.\n",
    );
    let _ = writeln!(out, "\n{}", times(&now.times, &baseline.times));
    changed_clips(&mut out, run, baseline);
    out.push_str(ATTRIBUTION);
    out
}

/// `baseline` with only the clips heard under each condition that `other` has too.
fn shared(baseline: &Baseline, other: &Baseline) -> Baseline {
    let mut clips = baseline.clips.clone();
    for (clip, conditions) in &mut clips {
        conditions.retain(|condition, _| other.clips.get(clip).is_some_and(|other| other.contains_key(condition)));
    }
    clips.retain(|_, conditions| !conditions.is_empty());
    Baseline { clips, ..baseline.clone() }
}

/// The errors outside the terms as a share of the words there, with how far it moved from `then`'s.
fn rate(now: &Counts, then: &Counts) -> String {
    let (rate, was) = (percent(now.errors(), now.words), percent(then.errors(), then.words));
    let delta = rate - was;
    if delta.abs() < 0.05 { format!("{rate:.1}%") } else { format!("{rate:.1}% ({delta:+.1})") }
}

/// How far `now` moved from `was`, if it did.
fn moved(now: usize, was: usize) -> String {
    match now.cmp(&was) {
        std::cmp::Ordering::Equal => String::new(),
        std::cmp::Ordering::Greater => format!(" (+{})", now - was),
        std::cmp::Ordering::Less => format!(" (−{})", was - now),
    }
}

#[expect(clippy::cast_precision_loss, reason = "counts of words, far below 2^52")]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 { 0.0 } else { 100.0 * part as f64 / whole as f64 }
}

fn times(now: &Times, then: &Times) -> String {
    let machines = if now.machine == then.machine {
        format!("Both runs' times are from {}.", now.machine)
    } else {
        format!("This run's times are from {}, and the baseline's from {}.", now.machine, then.machine)
    };
    format!(
        "Each clip's first hearing, among others, which the other two reuse the encodings of: from the end of a clip \
         to its last final text, {} ms at the median and {} ms at the 90th percentile, against {} and {}. The engine \
         took {} ms for each second of audio, against {}. {machines}",
        now.median_after_ms,
        now.p90_after_ms,
        then.median_after_ms,
        then.p90_after_ms,
        now.engine_ms_per_s,
        then.engine_ms_per_s,
    )
}

/// Lists the clips whose terms right or unsaid words written changed, with what's written now and how it differs
/// from what was said.
fn changed_clips(out: &mut String, run: &Run, baseline: &Baseline) {
    let now = run.baseline();
    let (shared_now, shared_then) = (shared(&now, baseline), shared(baseline, &now));
    let (missing, gone) = (pairs(&now) - pairs(&shared_now), pairs(baseline) - pairs(&shared_then));
    if missing + gone > 0 {
        let _ = writeln!(
            out,
            "\n{missing} clips under a condition aren't in the baseline, and {gone} in it weren't heard."
        );
    }
    let moved = |now: &Counts, then: &Counts| now.right != then.right || now.unsaid != then.unsaid;
    let changed: Vec<Changed> = shared_now
        .clips
        .iter()
        .flat_map(|(clip, conditions)| conditions.iter().map(move |(condition, both)| (clip, *condition, *both)))
        .filter_map(|(clip, condition, now)| {
            let then = shared_then.clips[clip][&condition];
            (moved(&now.final_text, &then.final_text) || moved(&now.live, &then.live)).then(|| Changed {
                clip: clip.clone(),
                condition,
                now,
                then,
            })
        })
        .collect();
    if changed.is_empty() {
        out.push_str("\nNo clip's terms right or unsaid hints written changed.\n");
        return;
    }
    let _ = writeln!(
        out,
        "\n{} clips under a condition changed their terms right or unsaid hints written:\n",
        changed.len()
    );
    list(out, run, changed);
}

/// A clip whose terms right or unsaid hints written changed under a condition.
struct Changed {
    clip: String,
    condition: Condition,
    now: Both,
    then: Both,
}

/// How many clips under a condition `baseline` has.
fn pairs(baseline: &Baseline) -> usize {
    baseline.clips.values().map(BTreeMap::len).sum()
}

/// Lists the first of `changed`, each with how its counts moved and how what's written now differs from what was
/// said.
fn list(out: &mut String, run: &Run, changed: Vec<Changed>) {
    let heard: BTreeMap<(&str, Condition), _> =
        run.heard.iter().map(|heard| ((heard.clip.as_str(), heard.condition), heard)).collect();
    let more = changed.len().saturating_sub(LISTED);
    for Changed { clip, condition, now, then } in changed.into_iter().take(LISTED) {
        let _ = writeln!(
            out,
            "- `{clip}`, {}: final {} → {} right and {} → {} unsaid, live {} → {} right and {} → {} unsaid",
            condition.name(),
            then.final_text.right,
            now.final_text.right,
            then.final_text.unsaid,
            now.final_text.unsaid,
            then.live.right,
            now.live.right,
            then.live.unsaid,
            now.live.unsaid,
        );
        if let Some(heard) = heard.get(&(clip.as_str(), condition)) {
            let _ = writeln!(out, "  - final: {}\n  - live: {}", heard.final_text.diff, heard.live.diff);
        }
    }
    if more > 0 {
        let _ = writeln!(out, "- and {more} more, in the run's `heard.jsonl`");
    }
}
