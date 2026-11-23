//! Scoring what was written against what was said.

use crate::clip::{Clip, Set, Source};
use crate::score::{Counts, diff, score};

fn clip(set: Set, text: &str, terms: &[&str], sound_alikes: &[&str], unrelated: &[&str]) -> Clip {
    let words = |words: &[&str]| words.iter().map(|word| (*word).to_string()).collect();
    Clip {
        set,
        id: "a-clip".to_string(),
        text: text.to_string(),
        terms: words(terms),
        sound_alikes: words(sound_alikes),
        unrelated: words(unrelated),
        audio: Source::Samples(Vec::new()),
    }
}

#[test]
fn a_term_is_right_only_spelled_with_its_own_capitals() {
    let said = clip(Set::TechSentences, "I pushed it to GitHub, then ran kubectl.", &["GitHub", "kubectl"], &[], &[]);

    assert_eq!(score(&said, "I pushed it to GitHub, then ran kubectl.").right, 2);
    assert_eq!(score(&said, "I pushed it to Github, then ran Kubectl.").right, 0);
    assert_eq!(score(&said, "I pushed it to git hub, then ran cube control.").right, 0);
}

#[test]
fn a_term_can_start_a_sentence_with_a_capital_it_doesnt_have() {
    let said = clip(Set::TechSentences, "Run it. kubectl applies it.", &["kubectl"], &[], &[]);

    assert_eq!(score(&said, "Run it. Kubectl applies it.").right, 1);
    assert_eq!(score(&said, "Run it and Kubectl applies it.").right, 0);
}

#[test]
fn a_term_said_twice_and_written_once_is_right_once() {
    let said = clip(Set::TechSentences, "Ask Ollama, then ask Ollama again.", &["Ollama"], &[], &[]);

    let counts = score(&said, "Ask Ollama, then ask Olama again.");

    assert_eq!((counts.terms, counts.right), (2, 1));
}

#[test]
fn librispeech_is_all_lower_case_so_case_doesnt_count() {
    let said = clip(Set::Librispeech, "the hawk's beak", &["hawk's"], &[], &[]);

    assert_eq!(score(&said, "The Hawk’s beak.").right, 1, "a curly apostrophe is a straight one");
}

#[test]
fn a_hinted_word_written_that_wasnt_said_counts_against_it() {
    let said = clip(Set::SoundAlikes, "Let's create a new branch.", &[], &["crate"], &["Proxmox"]);

    assert_eq!(score(&said, "Let's crate a new branch.").unsaid, 1);
    assert_eq!(score(&said, "Let's create a new branch on Proxmox.").unsaid, 1);
    assert_eq!(score(&said, "Let's create a new branch.").unsaid, 0);
}

#[test]
fn a_product_written_for_an_everyday_word_counts_only_with_its_capital() {
    let said = clip(Set::Capitals, "It needs a decent signal in here.", &[], &["Signal"], &[]);

    assert_eq!(score(&said, "It needs a decent Signal in here.").unsaid, 1);
    assert_eq!(score(&said, "It needs a decent signal in here.").unsaid, 0);
}

/// At a sentence's start, `Signal` can't be told from "signal", but `Claude` can be from "cloud".
#[test]
fn a_product_at_a_sentences_start_counts_only_if_it_isnt_a_word_said_with_a_capital() {
    let said = clip(Set::Capitals, "Use the cloud. Then signal us.", &[], &["Signal", "Claude"], &[]);

    assert_eq!(score(&said, "Use the cloud. Signal us.").unsaid, 0);
    assert_eq!(score(&said, "Use the Claude. Then signal us.").unsaid, 1);
    assert_eq!(score(&said, "Claude. Then signal us.").unsaid, 1);
}

#[test]
fn the_errors_are_in_the_words_outside_the_terms() {
    let said = clip(Set::TechSentences, "Please install bitsandbytes on the box.", &["bitsandbytes"], &[], &[]);

    let counts = score(&said, "Please install bits and bytes on a box today.");

    assert_eq!(
        counts,
        Counts { terms: 1, right: 0, unsaid: 0, words: 5, swapped: 1, left_out: 0, put_in: 3 },
        "of the words written for the term, none is an error outside it but the two put in, and \"a\" is swapped for \
         \"the\" and \"today\" put in"
    );
}

#[test]
fn a_hinted_word_put_in_isnt_also_an_error_outside_the_terms() {
    let said = clip(Set::Librispeech, "the stew was hot", &[], &[], &["mutton"]);

    let counts = score(&said, "the mutton stew was hot");

    assert_eq!((counts.unsaid, counts.put_in), (1, 0));
}

#[test]
fn a_diff_shows_each_word_swapped_left_out_or_put_in() {
    assert_eq!(
        diff("The cat sat on the mat.", "the bat sat on mat, today"),
        "the [cat→bat] sat on [-the] mat [+today]"
    );
    assert_eq!(diff("", "something"), "[+something]");
}

#[test]
fn a_term_with_a_capital_leaves_the_everyday_word_among_the_others() {
    let said = clip(Set::Capitals, "Let's go and write it in Go.", &["Go"], &[], &[]);

    let counts = score(&said, "Let's go and write it in Go.");

    assert_eq!(
        (counts.terms, counts.right, counts.words),
        (1, 1, 6),
        "\"go\" isn't the term, so it's a word of the others"
    );
}

#[test]
fn a_term_inside_a_longer_one_said_once_counts_once() {
    let said = clip(Set::TechSentences, "Port it to React Native, then React.", &["React", "React Native"], &[], &[]);

    let counts = score(&said, "Port it to React Native, then React.");

    assert_eq!((counts.terms, counts.right), (2, 2), "one React Native, and one React on its own");
}
