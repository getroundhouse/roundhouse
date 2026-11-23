//! Which clips a run hears, and which words each condition hints.

use crate::clip::{Clip, Condition, Set, Source};
use crate::sets::{spread, unrelated};

#[test]
fn a_run_hears_clips_spread_evenly_through_a_set() {
    assert_eq!(spread((0..10).collect(), Some(5)), [0, 2, 4, 6, 8]);
    assert_eq!(spread((0..10).collect(), Some(3)), [0, 3, 6]);
    assert_eq!(spread((0..3).collect(), Some(5)), [0, 1, 2], "all of them when there aren't more");
    assert_eq!(spread((0..3).collect(), None), [0, 1, 2]);
}

#[test]
fn each_clip_has_unrelated_words_of_its_own_on_every_run() {
    let pool = ["Ansible", "Grafana", "Jellyfin", "Tailscale", "Typst"];

    let first = unrelated("clip-1", pool.into_iter(), 3);

    assert_eq!(first.len(), 3);
    assert_eq!(first, unrelated("clip-1", pool.into_iter(), 3));
    assert_ne!(first, unrelated("clip-2", pool.into_iter(), 3), "another clip's are in another order");
}

#[test]
fn each_condition_hints_more_than_the_last() {
    let words = |words: &[&str]| words.iter().map(|word| (*word).to_string()).collect::<Vec<_>>();
    let clip = Clip {
        set: Set::SoundAlikes,
        id: "a-clip".to_string(),
        text: "Create a Pulumi stack.".to_string(),
        terms: words(&["Pulumi"]),
        sound_alikes: words(&["crate"]),
        unrelated: words(&["Grafana"]),
        audio: Source::Samples(Vec::new()),
    };

    assert!(clip.hint_words(Condition::NoHints).is_empty());
    assert_eq!(clip.hint_words(Condition::Said), ["Pulumi", "crate"]);
    assert_eq!(clip.hint_words(Condition::AmongOthers), ["Pulumi", "crate", "Grafana"]);
}
